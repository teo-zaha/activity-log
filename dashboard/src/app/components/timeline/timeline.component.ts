import {
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  afterRenderEffect,
  computed,
  inject,
  signal,
  viewChild,
  viewChildren,
} from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import { resizeObserver } from '@signality/core/observers/resize-observer'
import { EmptyStateComponent } from '../empty-state/empty-state.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityService } from '../../services/activity.service'
import { SearchBarComponent } from '../search-bar/search-bar.component'
import type { EntryUiAdapter } from '../../providers/types'
import { dayIndexOf } from '../../utils/day-index'
import { injectMinuteClock } from '../../utils/clock'
import { dayKeyOf } from '../../utils/local-day'
import { AutoAnimateDirective } from '../auto-animate/auto-animate.directive'
import { DayIndexComponent } from './day-index/day-index.component'
import { timelineEmptyState } from './timeline-empty-state'
import { TimelineRows } from './timeline-rows'

/** How many frames after a jump its landing is corrected, see `jumpTo`. */
const ALIGN_FRAMES = 4
/** How far below the sticky day header the line that picks the active day runs. */
const TRACK_LINE_GAP = 2

@Component({
  selector: 'app-timeline',
  imports: [
    EmptyStateComponent,
    ButtonComponent,
    SearchBarComponent,
    DayIndexComponent,
    AutoAnimateDirective,
  ],
  templateUrl: './timeline.component.html',
  styleUrl: './timeline.component.scss',
})
export class TimelineComponent {
  protected readonly activity = inject(ActivityService)

  /** Row widths (in %) for the loading skeleton. */
  protected readonly skeletonGroups = [
    [72, 54, 64],
    [58, 80],
  ]

  protected readonly summary = computed(() => {
    const groups = this.activity.groupedByDay()
    const events = groups.reduce((total, group) => total + group.entries.length, 0)
    const query = this.activity.searchQuery().trim()
    const base = `${events} ${events === 1 ? 'event' : 'events'} across ${groups.length} ${groups.length === 1 ? 'day' : 'days'}`
    return query ? `${base} matching “${query}”` : base
  })

  protected readonly emptyState = computed(() =>
    timelineEmptyState({
      query: this.activity.searchQuery().trim(),
      status: this.activity.collectionStatus(),
      hasSources: this.activity.providers().length > 0,
      anyShown: this.activity.activeConnections().size > 0,
    })
  )

  private readonly clock = injectMinuteClock()
  /** Today, which changes at midnight: the timeline stays open all session. */
  private readonly todayKey = computed(() => dayKeyOf(this.clock()))

  private readonly rows = new TimelineRows()
  /** The list, ready to draw: the template binds plain values, not per-row function calls. */
  protected readonly days = computed(() =>
    this.rows.days(this.activity.groupedByDay(), this.todayKey())
  )

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')
  private readonly content = viewChild.required<ElementRef<HTMLElement>>('content')
  private readonly sections = viewChildren<ElementRef<HTMLElement>>('daySection')
  /** The sections by day, for jumps: they look a day up on every frame they correct. */
  private readonly sectionsByDay = computed(
    () =>
      new Map(
        this.sections().map(({ nativeElement }) => [
          nativeElement.getAttribute('data-day') ?? '',
          nativeElement,
        ])
      )
  )

  /** Scroller height and the sticky day header offset, re-read whenever the scroller resizes. */
  private readonly viewport = signal(
    { height: 0, headerTop: 0, overflows: false },
    {
      equal: (a, b) =>
        a.height === b.height && a.headerTop === b.headerTop && a.overflows === b.overflows,
    }
  )
  /**
   * Days on or near the screen. Only their lists animate entries coming and going: AutoAnimate
   * keeps an observer and a timer for every row of a list it animates.
   */
  protected readonly nearDays = signal<ReadonlySet<string>>(new Set(), {
    equal: (a, b) => a.size === b.size && [...a].every(day => b.has(day)),
  })
  /** The day whose header is stuck at the top, as last seen by the observer or set by a jump. */
  private readonly trackedDay = signal<string | null>(null)
  /** After a jump the chosen day stays active (the last days may never reach the header line). */
  private pinned = false
  /** The frame that next re-aligns a jumped-to day, see `jumpTo`. */
  private alignFrame = 0
  /**
   * Where the list was scrolled to. The router keeps the timeline while a calendar view shows
   * (`TimelineReuseStrategy`), but leaving the document resets the scroller: it is put back here.
   */
  private lastScrollTop = 0
  private detached = false
  /**
   * False from leaving the document until the timeline is back and measured. Lists do not animate
   * meanwhile: AutoAnimate would measure their rows out of the document (as 0 × 0 at the corner)
   * and fly them in from there, so it starts afresh once the list is back.
   */
  protected readonly attached = signal(true)

  /** The side day index, from the oldest to the newest day the list shows. */
  protected readonly dayIndex = computed(() =>
    dayIndexOf(
      new Map(this.activity.groupedByDay().map(group => [group.date, group.entries.length])),
      this.todayKey()
    )
  )

  protected readonly showDayIndex = computed(
    () => this.dayIndex().length > 0 && this.viewport().overflows
  )

  protected readonly activeDay = computed(() => {
    const groups = this.activity.groupedByDay()
    const tracked = this.trackedDay()
    return groups.some(group => group.date === tracked) ? tracked : (groups[0]?.date ?? null)
  })

  constructor() {
    const measure = (): void => {
      const scroller = this.scroller().nativeElement
      if (!scroller.isConnected) {
        this.detached = true
        this.attached.set(false)
        return
      }
      if (this.detached) {
        // Back in the document, before it is painted: where the list was left.
        this.detached = false
        scroller.scrollTop = this.lastScrollTop
        // AutoAnimate may have measured rows while they were out of the document; it measures a
        // list's rows again when an animation inside an element holding the list ends.
        scroller.dispatchEvent(new Event('animationend'))
        this.attached.set(true)
      }
      const headerTop = parseFloat(getComputedStyle(this.host).getPropertyValue('--day-header-top'))
      this.viewport.set({
        height: scroller.clientHeight,
        headerTop: Number.isFinite(headerTop) ? headerTop : 0,
        overflows: scroller.scrollHeight > scroller.clientHeight + 1,
      })
    }
    afterNextRender(measure)
    // The content box changes size with the list, the scroller with the window.
    resizeObserver([this.scroller, this.content], measure)

    // Any scroll the user starts releases a jumped-to day; presses and keys on the rail do not.
    const release = (event: Event): void => {
      const fromRail = event.target instanceof Element && event.target.closest('app-day-index')
      if (event.type === 'wheel' || !fromRail) this.pinned = false
    }
    for (const type of ['wheel', 'touchstart', 'keydown', 'pointerdown']) {
      listener.passive(this.scroller, type, release)
    }
    listener.passive(this.scroller, 'scroll', () => {
      if (this.detached) return
      this.lastScrollTop = this.scroller().nativeElement.scrollTop
      // Near the end the line slides, which no section crossing tells the observer below.
      this.trackActiveDay()
    })
    inject(DestroyRef).onDestroy(() => cancelAnimationFrame(this.alignFrame))

    // Without scroll anchoring (WebKit), a day above the screen that is drawn for the first time,
    // its estimated height turning real, would move the list under the reader: the scroll
    // position takes up the difference instead. Hand-written like the observers below.
    if (typeof CSS !== 'undefined' && !CSS.supports('overflow-anchor', 'auto')) {
      const heights = new WeakMap<Element, number>()
      afterRenderEffect(onCleanup => {
        const sections = this.sections()
        const scroller = this.scroller().nativeElement
        const observer = new ResizeObserver(entries => {
          if (!scroller.isConnected) return
          const line = scroller.getBoundingClientRect().top + this.viewport().headerTop
          let shift = 0
          for (const entry of entries) {
            const height = entry.borderBoxSize[0]?.blockSize ?? entry.contentRect.height
            const before = heights.get(entry.target)
            heights.set(entry.target, height)
            if (before !== undefined && entry.target.getBoundingClientRect().bottom <= line) {
              shift += height - before
            }
          }
          if (shift) scroller.scrollTop += shift
        })
        for (const section of sections)
          observer.observe(section.nativeElement, { box: 'border-box' })
        onCleanup(() => observer.disconnect())
      })
    }

    // Hand-written like the observer below: the sections change with the list.
    afterRenderEffect(onCleanup => {
      const sections = this.sections()
      const near = new Set<string>()
      const observer = new IntersectionObserver(
        records => {
          for (const record of records) {
            const day = record.target.getAttribute('data-day')
            if (!day) continue
            if (record.isIntersecting) near.add(day)
            else near.delete(day)
          }
          this.nearDays.set(new Set(near))
        },
        // Half a screen above and below, so a day animates before it scrolls in.
        { root: this.scroller().nativeElement, rootMargin: '50% 0px' }
      )
      for (const section of sections) observer.observe(section.nativeElement)
      onCleanup(() => observer.disconnect())
    })

    // The active day is the section crossing a 1 px line just under the sticky day header. The
    // observer only says when a section crosses the line; the day is then read from where the
    // sections are, since a fast scroll can skip days and leave the line where none is (the
    // list's top padding). Hand-written: Signality's `intersectionObserver` takes a fixed list
    // of targets, and these sections change with the list and are observed only while the day
    // index shows.
    afterRenderEffect(onCleanup => {
      const sections = this.sections()
      const { height, headerTop } = this.viewport()
      if (!this.showDayIndex() || height === 0) return
      const line = headerTop + TRACK_LINE_GAP
      const observer = new IntersectionObserver(() => this.trackActiveDay(), {
        root: this.scroller().nativeElement,
        rootMargin: `-${line}px 0px -${Math.max(0, height - line - 1)}px 0px`,
      })
      for (const section of sections) observer.observe(section.nativeElement)
      onCleanup(() => observer.disconnect())
    })
  }

  /**
   * Lands the day's section on the sticky header line, without smooth scrolling. Days off screen
   * have estimated heights until they are drawn, and the jump draws the days around the target:
   * the landing is corrected over the next frames, until the user scrolls on their own.
   */
  protected jumpTo(day: string): void {
    if (!this.sectionOf(day)) return
    this.pinned = true
    this.trackedDay.set(day)
    cancelAnimationFrame(this.alignFrame)
    this.alignTo(day, ALIGN_FRAMES)
  }

  private alignTo(day: string, frames: number): void {
    const section = this.sectionOf(day)
    if (!section || !this.pinned) return
    const scroller = this.scroller().nativeElement
    const offset =
      section.getBoundingClientRect().top -
      scroller.getBoundingClientRect().top -
      this.viewport().headerTop
    if (Math.abs(offset) >= 1) {
      const before = scroller.scrollTop
      scroller.scrollTop += offset
      // At either end of the list the day cannot reach the line: nothing left to correct.
      if (scroller.scrollTop === before) return
    }
    // Hand-written: Signality has no animation-frame scheduling.
    if (frames > 0) this.alignFrame = requestAnimationFrame(() => this.alignTo(day, frames - 1))
  }

  protected focusFirstEntry(day: string): void {
    this.sectionOf(day)?.querySelector<HTMLElement>('.entry')?.focus()
  }

  /**
   * Sets the active day from the line under the sticky day header. The last days can never scroll
   * up to that line: over the last stretch of scrolling, as long as the screen below the line,
   * the line slides down to the bottom edge, so each of them becomes active in turn and the last
   * one at the end of the list.
   */
  private trackActiveDay(): void {
    if (this.pinned || !this.showDayIndex()) return
    const scroller = this.scroller().nativeElement
    const height = scroller.clientHeight
    const line = this.viewport().headerTop + TRACK_LINE_GAP
    const slideZone = Math.min(height - line, scroller.scrollHeight - height)
    const remaining = scroller.scrollHeight - height - scroller.scrollTop
    const slide =
      slideZone > 0 && remaining < slideZone
        ? (height - 1 - line) * (1 - Math.max(0, remaining) / slideZone)
        : 0
    const day = this.dayAt(scroller.getBoundingClientRect().top + line + slide)
    if (day) this.trackedDay.set(day)
  }

  /**
   * The day at viewport height `y`: the first section, top to bottom, that reaches below it. Above
   * the first section that is the first day, past the last the last one.
   */
  private dayAt(y: number): string | null {
    const sections = this.sections()
    let low = 0
    let high = sections.length - 1
    while (low < high) {
      const middle = (low + high) >> 1
      if (sections[middle].nativeElement.getBoundingClientRect().bottom > y) high = middle
      else low = middle + 1
    }
    return sections[low]?.nativeElement.getAttribute('data-day') ?? null
  }

  private sectionOf(day: string): HTMLElement | undefined {
    return this.sectionsByDay().get(day)
  }

  protected selectEntry(entry: EntryUiAdapter): void {
    void this.activity.openEvent(entry.trackKey)
  }
}
