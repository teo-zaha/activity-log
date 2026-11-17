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
import { MatIconModule } from '@angular/material/icon'
import { ActivityService } from '../../services/activity.service'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SearchBarComponent } from '../search-bar/search-bar.component'
import type { EntryUiAdapter } from '../../providers/types'
import { buildDayIndex } from '../../utils/day-index'
import { longDayLabel, relativeDayLabel, todayKey } from '../../utils/local-day'
import { DayIndexComponent } from './day-index/day-index.component'

interface TimelineEmptyState {
  readonly icon: string
  readonly title: string
  readonly message: string
  readonly action: 'clear-search' | null
}

@Component({
  selector: 'app-timeline',
  imports: [MatIconModule, ProviderIconComponent, SearchBarComponent, DayIndexComponent],
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

  protected readonly emptyState = computed<TimelineEmptyState>(() => {
    const query = this.activity.searchQuery().trim()
    const status = this.activity.collectionStatus()
    if (status.kind === 'timedOut' || status.kind === 'failed') {
      return {
        icon: 'cloud_off',
        title: 'No activity loaded',
        message:
          'The last collection did not finish. Retry above, or check source status in Settings.',
        action: null,
      }
    }
    if (query) {
      return {
        icon: 'search_off',
        title: `No matches for “${query}”`,
        message: 'Try a different term, or search by repo, branch, ticket key or author.',
        action: 'clear-search',
      }
    }
    if (this.activity.providers().length === 0) {
      return {
        icon: 'power_off',
        title: 'No sources are ready',
        message: 'Open Settings to see why each source is unavailable and configure it.',
        action: null,
      }
    }
    if (this.activity.activeConnections().size === 0) {
      return {
        icon: 'filter_alt_off',
        title: 'All sources are hidden',
        message: 'Turn on at least one source in the sidebar to see its activity.',
        action: null,
      }
    }
    return {
      icon: 'event_busy',
      title: 'Nothing recorded in this range',
      message: 'Pick a wider date range and collect again.',
      action: null,
    }
  })

  private readonly todayKey = todayKey()

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')
  private readonly content = viewChild.required<ElementRef<HTMLElement>>('content')
  private readonly sections = viewChildren<ElementRef<HTMLElement>>('daySection')

  /** Scroller height and the sticky day header offset, re-read whenever the scroller resizes. */
  private readonly viewport = signal(
    { height: 0, headerTop: 0, overflows: false },
    {
      equal: (a, b) =>
        a.height === b.height && a.headerTop === b.headerTop && a.overflows === b.overflows,
    }
  )
  /** The day whose header is stuck at the top, as last seen by the observer or set by a jump. */
  private readonly trackedDay = signal<string | null>(null)
  /** After a jump the chosen day stays active (the last days may never reach the header line). */
  private pinned = false

  /** Every day of the selected range, for the side day index. Empty below two days with events. */
  protected readonly dayIndex = computed(() => {
    const groups = this.activity.groupedByDay()
    if (groups.length < 2) return []
    const newest = groups[0].date
    const oldest = groups[groups.length - 1].date
    const from = this.activity.dateFrom()
    const to = this.activity.dateTo()
    const counts = new Map(groups.map(group => [group.date, group.entries.length]))
    return buildDayIndex(
      from && from < oldest ? from : oldest,
      to && to > newest ? to : newest,
      counts,
      this.todayKey
    )
  })

  protected readonly showDayIndex = computed(
    () => this.dayIndex().length > 0 && this.viewport().overflows
  )

  protected readonly activeDay = computed(() => {
    const groups = this.activity.groupedByDay()
    const tracked = this.trackedDay()
    return groups.some(group => group.date === tracked) ? tracked : (groups[0]?.date ?? null)
  })

  constructor() {
    const destroyRef = inject(DestroyRef)

    afterNextRender(() => {
      const scroller = this.scroller().nativeElement
      const measure = (): void => {
        const headerTop = parseFloat(
          getComputedStyle(this.host).getPropertyValue('--day-header-top')
        )
        this.viewport.set({
          height: scroller.clientHeight,
          headerTop: Number.isFinite(headerTop) ? headerTop : 0,
          overflows: scroller.scrollHeight > scroller.clientHeight + 1,
        })
      }
      measure()
      // The content box changes size with the list, the scroller with the window.
      const resize = new ResizeObserver(measure)
      resize.observe(scroller)
      resize.observe(this.content().nativeElement)

      // Any scroll the user starts releases a jumped-to day; presses and keys on the rail do not.
      const release = (event: Event): void => {
        const fromRail = event.target instanceof Element && event.target.closest('app-day-index')
        if (event.type === 'wheel' || !fromRail) this.pinned = false
      }
      const events = ['wheel', 'touchstart', 'keydown', 'pointerdown']
      for (const type of events) scroller.addEventListener(type, release, { passive: true })

      destroyRef.onDestroy(() => {
        resize.disconnect()
        for (const type of events) scroller.removeEventListener(type, release)
      })
    })

    // The active day is the section crossing a 1 px line just under the sticky day header.
    afterRenderEffect(onCleanup => {
      const sections = this.sections()
      const { height, headerTop } = this.viewport()
      if (!this.showDayIndex() || height === 0) return
      const line = headerTop + 2
      const observer = new IntersectionObserver(
        records => {
          if (this.pinned) return
          for (const record of records) {
            const day = record.isIntersecting ? record.target.getAttribute('data-day') : null
            if (day) this.trackedDay.set(day)
          }
        },
        {
          root: this.scroller().nativeElement,
          rootMargin: `-${line}px 0px -${Math.max(0, height - line - 1)}px 0px`,
        }
      )
      for (const section of sections) observer.observe(section.nativeElement)
      onCleanup(() => observer.disconnect())
    })
  }

  /** Lands the day's section on the sticky header line, without smooth scrolling. */
  protected jumpTo(day: string): void {
    const section = this.sectionOf(day)
    if (!section) return
    const scroller = this.scroller().nativeElement
    scroller.scrollTop +=
      section.getBoundingClientRect().top -
      scroller.getBoundingClientRect().top -
      this.viewport().headerTop
    this.pinned = true
    this.trackedDay.set(day)
  }

  protected focusFirstEntry(day: string): void {
    this.sectionOf(day)?.querySelector<HTMLElement>('.entry')?.focus()
  }

  private sectionOf(day: string): HTMLElement | undefined {
    return this.sections()
      .map(section => section.nativeElement)
      .find(section => section.getAttribute('data-day') === day)
  }

  protected selectEntry(entry: EntryUiAdapter): void {
    this.activity.selectedEntries.set(null)
    this.activity.selectedEntry.set(entry)
  }

  protected relativeDay(dateStr: string): string | null {
    return relativeDayLabel(dateStr, this.todayKey)
  }

  protected readonly formatDate = longDayLabel

  protected entryAriaLabel(entry: EntryUiAdapter): string {
    return `Open ${entry.badge.label} event: ${entry.title} at ${entry.timeLabel}`
  }
}
