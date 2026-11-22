import {
  type AnimationCallbackEvent,
  Component,
  ElementRef,
  type Signal,
  afterNextRender,
  afterRenderEffect,
  computed,
  inject,
  signal,
  untracked,
  viewChild,
  viewChildren,
} from '@angular/core'
import { LiveAnnouncer } from '@angular/cdk/a11y'
import { resizeObserver } from '@signality/core/observers/resize-observer'
import { debounced } from '@signality/core/reactivity/debounced'
import { watcher } from '@signality/core/reactivity/watcher'
import { injectVirtualizer } from '@tanstack/angular-virtual'
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
import { dayKeyOf, type DayKey } from '../../utils/local-day'
import { AutoAnimateDirective } from '../auto-animate/auto-animate.directive'
import { DayIndexComponent } from './day-index/day-index.component'
import { type DayPositions, trackActiveDay } from './active-day'
import { keepPlace, type PlacePositions } from './keep-place'
import { followFeedKeys } from './feed-keys'
import { trackAttachment } from './attachment'
import { timelineEmptyState, timelineEmptyView } from './timeline-empty-state'
import { type TimelineDay, TimelineRows } from './timeline-rows'
import { indicatorDistance, pullToRefresh, type PullState } from './pull-to-refresh'
import { TopLayer } from '../../layouts/shell/top-layer.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import { restoresScroll } from '../../services/views-scroller.service'

/** The first days rise in as the list is first drawn; later ones, scrolled to, do not. */
const RISING_DAYS = 6

/** A day's header and padding, before the day is first drawn and measured. */
const DAY_CHROME_ESTIMATE = 54
/** An entry's row before it is first drawn; on a phone its title wraps to two lines. */
const ENTRY_ESTIMATE = { wide: 66, phone: 74 } as const

/** The day the virtual list asks about: it asks only for indexes below the count it was given. */
function dayAt(days: readonly TimelineDay[], index: number): TimelineDay {
  const day = days[index]
  if (day === undefined) throw new RangeError(`No day ${index} of ${days.length}`)
  return day
}

/** A day drawn, with where the virtual list places it. */
interface DrawnDay {
  readonly day: TimelineDay
  readonly index: number
}

/** A length custom property, in pixels; `fallback` where it is not one. */
function pixelsOf(style: CSSStyleDeclaration, property: string, fallback: number): number {
  const value = parseFloat(style.getPropertyValue(property))
  return Number.isFinite(value) ? value : fallback
}

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
    timelineEmptyView(
      timelineEmptyState({
        query: this.viewState.searchQuery().trim(),
        status: this.data.collectionStatus(),
        hasSources: this.data.anySourceReady(),
        anyShown: this.viewState.activeConnections().size > 0,
        coverage: this.selection.rangeCoverage(),
      })
    )
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
  private readonly isPhone = inject(BreakpointState).phone
  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')
  private readonly content = viewChild.required<ElementRef<HTMLElement>>('content')
  private readonly controls = viewChild.required<ElementRef<HTMLElement>>('controls')
  private readonly list = viewChild<ElementRef<HTMLElement>>('list')
  private readonly sections = viewChildren<ElementRef<HTMLElement>>('daySection')

  /**
   * The scroller's height, the sticky day header's offset and where the list starts in the
   * scroller's content, re-read whenever the scroller or what is above the list resizes.
   */
  private readonly viewport = signal(
    { height: 0, headerTop: 0, listTop: 0, overflows: false },
    {
      equal: (a, b) =>
        a.height === b.height &&
        a.headerTop === b.headerTop &&
        a.listTop === b.listTop &&
        a.overflows === b.overflows,
    }
  )

  /**
   * Only the days on or near the screen are drawn, each measured once drawn and estimated until
   * then (`DAY_CHROME_ESTIMATE`, `ENTRY_ESTIMATE`). A day is drawn whole, its header sticking
   * within it, so a long range keeps few rows in the document, also while another view shows. A
   * day drawn with other rows is another item (`TimelineDay.key`), measured afresh.
   */
  protected readonly virtualDays = injectVirtualizer<HTMLElement, HTMLElement>(() => {
    const days = this.days()
    const { headerTop, listTop } = this.viewport()
    const entry = this.isPhone() ? ENTRY_ESTIMATE.phone : ENTRY_ESTIMATE.wide
    return {
      scrollElement: this.scroller(),
      count: days.length,
      estimateSize: index => DAY_CHROME_ESTIMATE + dayAt(days, index).rows.length * entry,
      getItemKey: index => dayAt(days, index).key,
      overscan: 2,
      scrollMargin: listTop,
      // A day scrolled to lands under the sticky search bar, as its header sticks there.
      scrollPaddingStart: headerTop,
    }
  })

  /**
   * The days drawn. The document keeps a day's elements by its date as its rows change, so
   * AutoAnimate animates them and the focus on an entry stays.
   */
  protected readonly drawn = computed<readonly DrawnDay[]>(() => {
    const days = this.days()
    return this.virtualDays
      .getVirtualItems()
      .flatMap(({ index }) => (index < days.length ? [{ day: dayAt(days, index), index }] : []))
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

  /** False while the router keeps the list out of the document. */
  protected readonly attached = trackAttachment(this.scroller)

  private readonly dayKeys = computed(() => this.days().map(day => day.date))

  /** Where the virtual list places the days, measured or estimated. */
  private readonly positions: DayPositions & PlacePositions = {
    indexAt: offset => this.virtualDays.getVirtualItemForOffset(offset)?.index ?? null,
    startOf: index => this.virtualDays.measurementsCache()[index]?.start ?? null,
    scrollTo: index => this.virtualDays.scrollToIndex(index, { align: 'start' }),
    scrollToOffset: offset => this.virtualDays.scrollToOffset(offset),
  }

  protected readonly activeDay = trackActiveDay({
    scroller: this.scroller,
    days: this.dayKeys,
    positions: this.positions,
    viewport: this.viewport,
    enabled: this.showDayIndex,
    attached: this.attached,
  })

  /** A pull at the top of the list collects the range on screen again, as Collect does. */
  protected readonly pull = this.pullToCollect()

  /** How far down the pull indicator shows. */
  protected readonly pullDistance = computed(() => indicatorDistance(this.pull()))

  constructor() {
    const measure = (): void => {
      const scroller = this.scroller().nativeElement
      if (!scroller.isConnected) return
      const style = getComputedStyle(this.host)
      const list = this.list()?.nativeElement
      this.viewport.set({
        height: scroller.clientHeight,
        headerTop: pixelsOf(style, '--day-header-top', 0),
        listTop: list
          ? list.getBoundingClientRect().top -
            scroller.getBoundingClientRect().top +
            scroller.scrollTop
          : 0,
        overflows: scroller.scrollHeight > scroller.clientHeight + 1,
      })
    }
    this.announceSettled()
    // A new navigation shows the list from its top, Back and Forward where it was left.
    restoresScroll(this.scroller)
    // Page Down and Page Up step through the days, as a feed's articles.
    followFeedKeys({
      scroller: this.scroller,
      count: () => this.days().length,
      scrollTo: index => this.positions.scrollTo(index),
    })
    // A collection or a filter that changes the days above keeps the day read where it is.
    keepPlace({
      scroller: this.scroller,
      days: this.dayKeys,
      positions: this.positions,
      line: () => this.viewport().headerTop,
      attached: this.attached,
    })
    afterNextRender(measure)
    // The content box changes size with the list, the scroller with the window, and the filter
    // above the list with its text size.
    resizeObserver([this.scroller, this.content, this.controls], measure)
    // Each drawn day is measured, and observed for the rows AutoAnimate or a collection adds.
    afterRenderEffect(() => {
      const sections = this.sections()
      for (const section of sections) this.virtualDays.measureElement(section.nativeElement)
      if (sections.length > 0) this.risen = true
    })
  }

  /** Whether the list was drawn with days: the days drawn after that do not rise in. */
  private risen = false

  /** `(animate.enter)` of a day: the first days rise in, without a lasting fill. */
  protected rise(event: AnimationCallbackEvent, index: number): void {
    if (this.risen || index >= RISING_DAYS) return
    const element = event.target
    element.classList.add('day-rise')
    void Promise.allSettled(element.getAnimations().map(animation => animation.finished)).then(() =>
      element.classList.remove('day-rise')
    )
  }

  /**
   * Reads the summary out once the filter has settled, as the URL takes it, not on every key: a
   * live region around it would be read at each keystroke. Nothing is read while a collection
   * runs, so only its result is, nor while the Timeline is not what the user is on: another view,
   * or the settings or the details over it (the router keeps the Timeline alive underneath).
   */
  private announceSettled(): void {
    const announcer = inject(LiveAnnouncer)
    const shown = computed(() =>
      !this.data.isLoading() && this.selection.groupedByDay().length > 0 ? this.summary() : null
    )
    const onScreen = computed(
      () =>
        this.viewState.currentView() === 'list' &&
        this.viewState.settings() === null &&
        !this.viewState.detailsOpen()
    )
    watcher(debounced(shown, inject(SEARCH_URL_DELAY_MS)), summary => {
      if (summary !== null && untracked(onScreen)) void announcer.announce(summary, 'polite')
    })
  }

  /**
   * Pull to refresh, for touch input only: offered while the Timeline is what the user is on, with
   * nothing over it, no collection running and a source to collect. A collection it starts, or a
   * refused one, shows as every collection does (`CollectionStatus`), which screen readers hear;
   * the indicator itself is hidden from them, and says once that releasing collects.
   */
  private pullToCollect(): Signal<PullState> {
    const layers = inject(TopLayer)
    const announcer = inject(LiveAnnouncer)
    return pullToRefresh({
      scroller: this.scroller,
      controls: this.controls,
      enabled: computed(
        () =>
          // Under another view the router holds the Timeline out of the document, and WebKit
          // still sends a touch that began on it its events.
          this.viewState.currentView() === 'list' &&
          layers.top() === null &&
          this.data.anySourceReady() &&
          !this.data.isLoading()
      ),
      refresh: () => void this.data.collectShownRange(),
      announce: () => void announcer.announce('Release to collect', 'polite'),
    })
  }

  protected collectOwnRange(): void {
    void this.data.collectShownRange()
  }

  /** The active day is under the line, so it is drawn. */
  protected focusFirstEntry(day: DayKey): void {
    this.sections()
      .find(section => section.nativeElement.dataset['day'] === day)
      ?.nativeElement.querySelector<HTMLElement>('.entry')
      ?.focus()
  }

  protected selectEntry(entry: EntryUiAdapter): void {
    void this.viewState.openEvent(entry.trackKey)
  }
}
