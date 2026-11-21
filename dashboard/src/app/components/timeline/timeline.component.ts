import {
  Component,
  ElementRef,
  afterNextRender,
  afterRenderEffect,
  computed,
  inject,
  signal,
  viewChild,
  viewChildren,
} from '@angular/core'
import { LiveAnnouncer } from '@angular/cdk/a11y'
import { resizeObserver } from '@signality/core/observers/resize-observer'
import { debounced } from '@signality/core/reactivity/debounced'
import { watcher } from '@signality/core/reactivity/watcher'
import { EmptyStateComponent } from '../empty-state/empty-state.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection } from '../../services/activity-selection.service'
import {
  ActivityViewStateService,
  SEARCH_URL_DELAY_MS,
} from '../../services/activity-view-state.service'
import { SearchBarComponent } from '../search-bar/search-bar.component'
import type { EntryUiAdapter } from '../../providers/types'
import { dayIndexOf } from '../../utils/day-index'
import { injectMinuteClock } from '../../utils/clock'
import { dayKeyOf } from '../../utils/local-day'
import { AutoAnimateDirective } from '../auto-animate/auto-animate.directive'
import { DayIndexComponent } from './day-index/day-index.component'
import { trackActiveDay } from './active-day'
import { keepScroll } from './keep-scroll'
import { timelineEmptyState } from './timeline-empty-state'
import { TimelineRows } from './timeline-rows'

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
  protected readonly selection = inject(ActivitySelection)
  protected readonly viewState = inject(ActivityViewStateService)
  protected readonly data = inject(ActivityDataService)

  /** Row widths (in %) for the loading skeleton. */
  protected readonly skeletonGroups = [
    [72, 54, 64],
    [58, 80],
  ]

  protected readonly summary = computed(() => {
    const groups = this.selection.groupedByDay()
    const events = groups.reduce((total, group) => total + group.entries.length, 0)
    const query = this.viewState.searchQuery().trim()
    const base = `${events} ${events === 1 ? 'event' : 'events'} across ${groups.length} ${groups.length === 1 ? 'day' : 'days'}`
    return query ? `${base} matching “${query}”` : base
  })

  protected readonly emptyState = computed(() =>
    timelineEmptyState({
      query: this.viewState.searchQuery().trim(),
      status: this.data.collectionStatus(),
      hasSources: this.data.providers().length > 0,
      anyShown: this.viewState.activeConnections().size > 0,
      coverage: this.selection.rangeCoverage(),
    })
  )

  private readonly clock = injectMinuteClock()
  /** Today, which changes at midnight: the timeline stays open all session. */
  private readonly todayKey = computed(() => dayKeyOf(this.clock()))

  private readonly rows = new TimelineRows()
  /** The list, ready to draw: the template binds plain values, not per-row function calls. */
  protected readonly days = computed(() =>
    this.rows.days(this.selection.groupedByDay(), this.todayKey())
  )

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
  /**
   * Days on or near the screen. Only their lists animate entries coming and going: AutoAnimate
   * keeps an observer and a timer for every row of a list it animates.
   */
  protected readonly nearDays = signal<ReadonlySet<string>>(new Set(), {
    equal: (a, b) => a.size === b.size && [...a].every(day => b.has(day)),
  })

  /** The side day index, from the oldest to the newest day the list shows. */
  protected readonly dayIndex = computed(() =>
    dayIndexOf(
      new Map(this.selection.groupedByDay().map(group => [group.date, group.entries.length])),
      this.todayKey()
    )
  )

  protected readonly showDayIndex = computed(
    () => this.dayIndex().length > 0 && this.viewport().overflows
  )

  protected readonly kept = keepScroll({
    scroller: this.scroller,
    sections: this.sections,
    headerTop: () => this.viewport().headerTop,
  })

  protected readonly activeDay = trackActiveDay({
    scroller: this.scroller,
    sections: this.sections,
    days: computed(() => this.selection.groupedByDay().map(group => group.date)),
    viewport: this.viewport,
    enabled: this.showDayIndex,
    attached: this.kept.attached,
  })

  constructor() {
    const measure = (): void => {
      const scroller = this.scroller().nativeElement
      if (!scroller.isConnected) return
      const headerTop = parseFloat(getComputedStyle(this.host).getPropertyValue('--day-header-top'))
      this.viewport.set({
        height: scroller.clientHeight,
        headerTop: Number.isFinite(headerTop) ? headerTop : 0,
        overflows: scroller.scrollHeight > scroller.clientHeight + 1,
      })
    }
    this.announceSettled()
    afterNextRender(measure)
    // The content box changes size with the list, the scroller with the window.
    resizeObserver([this.scroller, this.content], measure)

    // Hand-written: the sections change with the list, and Signality's `intersectionObserver`
    // takes a fixed list of targets.
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
  }

  /**
   * Reads the summary out once the filter has settled, as the URL takes it, not on every key: a
   * live region around it would be read at each keystroke.
   */
  private announceSettled(): void {
    const announcer = inject(LiveAnnouncer)
    const shown = computed(() => (this.selection.groupedByDay().length > 0 ? this.summary() : null))
    watcher(debounced(shown, inject(SEARCH_URL_DELAY_MS)), summary => {
      if (summary !== null) void announcer.announce(summary, 'polite')
    })
  }

  protected collectOwnRange(): void {
    void this.data.collectShownRange()
  }

  protected focusFirstEntry(day: string): void {
    this.activeDay.sectionOf(day)?.querySelector<HTMLElement>('.entry')?.focus()
  }

  protected selectEntry(entry: EntryUiAdapter): void {
    void this.viewState.openEvent(entry.trackKey)
  }
}
