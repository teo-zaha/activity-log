import {
  type AnimationCallbackEvent,
  Component,
  DestroyRef,
  ElementRef,
  Injector,
  Renderer2,
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
import { defaultRangeExtractor, injectVirtualizer } from '@tanstack/angular-virtual'
import { listener } from '@signality/core/browser/listener'
import { mediaQuery } from '@signality/core/browser/media-query'
import { EmptyStateComponent } from '../empty-state/empty-state.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection } from '../../services/activity-selection.service'
import {
  ActivityViewStateService,
  SEARCH_URL_DELAY_MS,
  type DayRange,
} from '../../services/activity-view-state.service'
import { SearchBarComponent } from '../search-bar/search-bar.component'
import type { EntryUiAdapter } from '../../providers/types'
import { dayIndexOf } from '../../utils/day-index'
import { injectMinuteClock } from '../../utils/clock'
import { dayKeyOf, type DayKey } from '../../utils/local-day'
import { DayIndexComponent } from './day-index/day-index.component'
import { type DayPositions, trackActiveDay } from './active-day'
import { keepPlace, type Placement } from './keep-place'
import { ListMotion, type PositionedItem } from '../list-motion/list-motion'
import { followFeedKeys } from './feed-keys'
import { trackAttachment } from './attachment'
import { timelineEmptyState, timelineEmptyView } from './timeline-empty-state'
import { TimelineRows } from './timeline-rows'
import { drawnDays, drawnIndexes, timelineItems } from './timeline-items'
import { indicatorDistance, pullToRefresh, type PullState } from './pull-to-refresh'
import { TopLayer } from '../../layouts/shell/top-layer.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import { restoresScroll } from '../../services/views-scroller.service'

/** The first days rise in as the list is first drawn; later ones, scrolled to, do not. */
const RISING_DAYS = 6

/** One-line row height including its separator, and the extra line on a phone. */
const ENTRY_HEIGHT = 66
const TITLE_LINE_HEIGHT = 20
const HEADER_ESTIMATE = 36
/** The article's bottom padding, including the room between adjacent days. */
const DAY_GAP = 18
/** Gives normal scrolling room to mount the next rows before they enter the viewport. */
const OVERSCAN = 8

/** The virtual list's own correction for days measured above the screen: `keepPlace` keeps it. */
const NO_ADJUSTMENT = (): boolean => false

/** A length custom property, in pixels; `fallback` where it is not one. */
function pixelsOf(style: CSSStyleDeclaration, property: string, fallback: number): number {
  const value = parseFloat(style.getPropertyValue(property))
  return Number.isFinite(value) ? value : fallback
}

@Component({
  selector: 'app-timeline',
  imports: [EmptyStateComponent, ButtonComponent, SearchBarComponent, DayIndexComponent],
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
  private readonly titleWraps = signal<(entry: EntryUiAdapter) => boolean>(() => false)
  private titleWidth = 0
  private readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')
  private readonly content = viewChild.required<ElementRef<HTMLElement>>('content')
  private readonly controls = viewChild.required<ElementRef<HTMLElement>>('controls')
  private readonly list = viewChild<ElementRef<HTMLElement>>('list')
  private readonly measured = viewChildren<ElementRef<HTMLElement>>('measured')
  private readonly injector = inject(Injector)
  private readonly ghosts = viewChild.required<ElementRef<HTMLElement>>('ghosts')
  private readonly motion = new ListMotion()
  private readonly renderer = inject(Renderer2)
  private readonly reduceMotion = mediaQuery('(prefers-reduced-motion: reduce)')
  private readonly settledQuery = debounced(this.viewState.searchQuery, inject(SEARCH_URL_DELAY_MS))

  /**
   * The scroller's height, the sticky day header's offset and where the list starts in the
   * scroller's content, re-read whenever the scroller or what is above the list resizes.
   */
  private readonly viewport = signal(
    { height: 0, width: 0, headerTop: 0, listTop: 0, afterList: 0 },
    {
      equal: (a, b) =>
        a.height === b.height &&
        a.width === b.width &&
        a.headerTop === b.headerTop &&
        a.listTop === b.listTop &&
        a.afterList === b.afterList,
    }
  )

  private readonly layout = computed(() => timelineItems(this.days()))
  /** An article or entry with focus must stay in the virtual window until focus leaves it. */
  private readonly focused = signal<string | null>(null)
  private readonly pendingFocus = signal<string | null>(null)

  protected readonly virtualRows = injectVirtualizer<HTMLElement, HTMLElement>(() => {
    const layout = this.layout()
    const { headerTop, listTop } = this.viewport()
    const wraps = this.isPhone() ? this.titleWraps() : null
    const keep = [this.focused(), this.pendingFocus()]
      .map(key => (key === null ? undefined : layout.indexes.get(key)))
      .filter(index => index !== undefined)
    return {
      scrollElement: this.scroller(),
      count: layout.items.length,
      getItemKey: (index): string => layout.items[index]!.key,
      estimateSize: (index): number => {
        const item = layout.items[index]!
        if (item.kind === 'row') {
          const first = layout.days[item.dayIndex]!.header + 1 === index
          const title = wraps?.(item.row.entry) ? TITLE_LINE_HEIGHT : 0
          // A tentative or free meeting's 18px tag is taller than the 17px compact badge.
          const busyTag = item.row.busy ? 1 : 0
          return ENTRY_HEIGHT + title + busyTag - (first ? 1 : 0)
        }
        return item.kind === 'header' ? HEADER_ESTIMATE : DAY_GAP
      },
      indexAttribute: 'data-item-index',
      // Register new nodes in the write phase, and take sizes from the observer after layout.
      // Supplying the previous size or estimate here avoids TanStack's synchronous size read.
      measureElement: (element, observed, virtualizer): number => {
        if (observed) return observed.borderBoxSize?.[0]?.blockSize ?? element.offsetHeight
        const index = virtualizer.indexFromElement(element)
        return (
          virtualizer.itemSizeCache.get(virtualizer.options.getItemKey(index)) ??
          virtualizer.options.estimateSize(index)
        )
      },
      overscan: OVERSCAN,
      rangeExtractor: (range): number[] =>
        drawnIndexes(layout, [...defaultRangeExtractor(range), ...keep]),
      scrollMargin: listTop,
      scrollPaddingStart: headerTop,
    }
  })

  protected readonly motionWidth = computed(() => this.viewport().width)

  protected readonly drawn = computed(() => {
    const window = this.virtualRows.getVirtualItems()
    return drawnDays(
      this.layout(),
      this.virtualRows.measurementsCache(),
      window,
      this.viewport().listTop
    )
  })

  private readonly extent = computed(() => {
    const { listTop, afterList, height } = this.viewport()
    return listTop + this.virtualRows.getTotalSize() + afterList - height
  })

  /** The side day index, from the oldest to the newest day the list shows. */
  protected readonly dayIndex = computed(() =>
    dayIndexOf(
      new Map(this.selection.groupedByDay().map(group => [group.date, group.entries.length])),
      this.todayKey()
    )
  )

  protected readonly showDayIndex = computed(() => this.dayIndex().length > 0 && this.extent() > 1)

  /** False while the router keeps the list out of the document. */
  protected readonly attached = trackAttachment(this.scroller)

  private readonly dayKeys = computed(() => this.days().map(day => day.date))

  /** Where the virtual list places the days, measured or estimated. */
  private readonly positions: DayPositions = {
    indexAt: offset => {
      const item = this.virtualRows.getVirtualItemForOffset(offset)
      return item === undefined ? null : (this.layout().items[item.index]?.dayIndex ?? null)
    },
    scrollTo: index => {
      const day = this.layout().days[index]
      if (day) {
        this.motion.reset()
        this.virtualRows.scrollToIndex(day.header, { align: 'start' })
      }
    },
  }

  protected readonly activeDay = trackActiveDay({
    scroller: this.scroller,
    days: this.dayKeys,
    positions: this.positions,
    viewport: computed(() => {
      const { height, headerTop } = this.viewport()
      return { height, headerTop, scrollHeight: height + Math.max(0, this.extent()) }
    }),
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
      this.measureTitle()
      this.viewport.set({
        height: scroller.clientHeight,
        width: scroller.clientWidth,
        headerTop: pixelsOf(style, '--day-header-top', 0),
        listTop: list
          ? list.getBoundingClientRect().top -
            scroller.getBoundingClientRect().top +
            scroller.scrollTop
          : 0,
        afterList: pixelsOf(getComputedStyle(this.content().nativeElement), 'padding-bottom', 0),
      })
    }
    this.announceSettled()
    // The row read stays where it is as what is above it changes; a new navigation shows the
    // list from its top, Back and Forward that row where it was left.
    const place = keepPlace({
      scroller: this.scroller,
      layout: this.layout,
      coordinates: () => {
        const maxScroll = this.extent()
        const measurements = this.virtualRows.measurementsCache()
        return {
          startOf: (index): number | undefined => measurements[index]?.start,
          indexAt: (offset): number | undefined =>
            this.virtualRows.getVirtualItemForOffset(offset)?.index,
          maxScroll,
          line: this.viewport().headerTop,
        }
      },
      placed: placement => this.animateFrame(placement),
    })
    restoresScroll(this.scroller, () => 0, place)
    inject(DestroyRef).onDestroy(() => this.motion.reset())
    followFeedKeys({
      scroller: this.scroller,
      count: () => this.days().length,
      focusDay: index => {
        const day = this.layout().days[index]
        if (day) this.focusItem(day.header)
      },
      tab: (target, step) => this.tab(target, step),
    })
    // A window blur can send focusout with no destination. Keep the item until a focusin
    // actually moves focus to another element, including one outside the scroller.
    listener(this.host.ownerDocument, 'focusin', (event: FocusEvent) =>
      this.followFocus(event.target)
    )
    const fonts = this.host.ownerDocument.fonts
    if (fonts) listener(fonts, 'loadingdone', () => this.measureTitle(true))
    afterNextRender({ read: measure })
    resizeObserver([this.scroller, this.content, this.controls], measure)
    afterRenderEffect({
      write: () => {
        this.virtualRows().shouldAdjustScrollPositionOnItemSizeChange = NO_ADJUSTMENT
        const measured = this.measured()
        this.virtualRows.measureElement(null)
        for (const item of measured) this.virtualRows.measureElement(item.nativeElement)
        if (measured.length > 0) this.risen = true
      },
    })
  }

  /** Records the committed virtual positions after keepPlace finishes scrolling. */
  private animateFrame(placement: Placement): void {
    const query = this.viewState.searchQuery()
    const layout = this.layout()
    const { headerTop, height } = this.viewport()
    const positions = this.virtualRows.measurementsCache()
    const items = new Map<string, PositionedItem>()
    for (const { nativeElement } of this.measured()) {
      const index = Number(nativeElement.dataset['itemIndex'])
      const item = layout.items[index]
      const bounds = positions[index]
      if (!item || !bounds) continue
      const end = positions[layout.days[item.dayIndex]!.gap]?.end
      items.set(item.key, {
        element: nativeElement,
        size: bounds.size,
        topAt: (scrollTop): number => {
          const top = bounds.start - scrollTop
          if (item.kind === 'row' || end === undefined) return top
          return Math.min(Math.max(top, headerTop), end - scrollTop - bounds.size)
        },
      })
    }
    this.motion.update({
      keys: layout.indexes,
      items,
      viewport: { top: headerTop, bottom: height },
      pauseOn: query,
      ...placement,
      revision: this.data.allEntries(),
      paused:
        !this.attached() ||
        !this.host.isConnected ||
        this.reduceMotion() ||
        this.settledQuery() !== query ||
        typeof this.host.animate !== 'function',
      ghost: (item, top) => this.makeGhost(item, top),
    })
  }

  /** The motion layer is outside the feed and is inert and hidden from assistive technology. */
  private makeGhost(item: PositionedItem, top: number): HTMLElement | null {
    const clone = item.element.cloneNode(true)
    if (!(clone instanceof HTMLElement)) return null
    clone.removeAttribute('data-item-index')
    clone.removeAttribute('aria-posinset')
    clone.removeAttribute('aria-setsize')
    clone.querySelector('[data-entry]')?.removeAttribute('data-entry')
    const element: HTMLElement = this.renderer.createElement('div')
    element.className = 'timeline-ghost'
    element.style.top = `${top}px`
    element.style.height = `${item.size}px`
    element.append(clone)
    this.ghosts().nativeElement.append(element)
    return element
  }

  /** The rendered title supplies its column width and font. ResizeObserver owns the final size. */
  private measureTitle(fontChanged = false): void {
    if (!this.isPhone()) {
      if (this.titleWidth) {
        this.titleWidth = 0
        this.virtualRows.measure()
      }
      return
    }
    const title = this.list()?.nativeElement.querySelector<HTMLElement>('.entry-title')
    if (!title) return
    const width = title.getBoundingClientRect().width
    if (!width || (width === this.titleWidth && !fontChanged)) return
    const context = this.host.ownerDocument.createElement('canvas').getContext('2d')
    if (!context) return
    const style = getComputedStyle(title)
    context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`
    this.titleWidth = width
    const cache = new WeakMap<EntryUiAdapter, boolean>()
    const words = new Intl.Segmenter(undefined, { granularity: 'word' })
    this.titleWraps.set((entry): boolean => {
      const known = cache.get(entry)
      if (known !== undefined) return known
      const line = entry.title.replace(/[\t\n\f\r ]+/g, ' ').replace(/^ | $/g, '')
      const wraps =
        context.measureText(line).width > width &&
        (line.includes(' ') || [...words.segment(line)].length > 1)
      cache.set(entry, wraps)
      return wraps
    })
    // Heights measured with the previous column width no longer describe these rows.
    this.virtualRows.measure()
  }

  private followFocus(target: EventTarget | null): void {
    const key =
      target instanceof HTMLElement && this.scroller().nativeElement.contains(target)
        ? (target.closest<HTMLElement>('[data-entry]')?.dataset['entry'] ??
          target.closest<HTMLElement>('article[data-day]')?.dataset['day'])
        : undefined
    this.focused.set(key ?? null)
  }

  /** Native Tab can handle two mounted neighbors. Bridge a gap only when the next one is absent. */
  private tab(target: HTMLElement, step: 1 | -1): boolean {
    const article = target.closest<HTMLElement>('article[data-day]')
    if (!article) return false
    const layout = this.layout()
    const key = target.dataset['entry'] ?? article.dataset['day']
    const index = key === undefined ? undefined : layout.indexes.get(key)
    if (index === undefined) return false
    for (let next = index + step; next >= 0 && next < layout.items.length; next += step) {
      if (layout.items[next]?.kind !== 'row') continue
      if (this.itemElement(next)) return false
      this.focusItem(next)
      return true
    }
    return false
  }

  private itemElement(index: number): HTMLElement | null {
    const root = this.scroller().nativeElement
    const node = root.querySelector<HTMLElement>(`[data-item-index="${index}"]`)
    return node?.matches('header')
      ? node.closest<HTMLElement>('article')
      : (node?.querySelector<HTMLElement>('.entry') ?? null)
  }

  private focusItem(index: number): void {
    const layout = this.layout()
    const item = layout.items[index]
    if (!item) return
    if (item.kind === 'header') {
      this.positions.scrollTo(item.dayIndex)
    } else {
      const positions = this.virtualRows.measurementsCache()
      const row = positions[index]
      const header = positions[layout.days[item.dayIndex]!.header]
      if (row && header) {
        this.motion.reset()
        this.virtualRows.scrollToOffset(row.start - header.size - this.viewport().headerTop)
      }
    }
    const drawn = this.itemElement(index)
    if (drawn) {
      drawn.focus({ preventScroll: true })
      return
    }
    this.pendingFocus.set(item.key)
    afterNextRender(
      {
        write: () => {
          if (this.pendingFocus() !== item.key) return
          const current = this.layout().indexes.get(item.key)
          if (current !== undefined) this.itemElement(current)?.focus({ preventScroll: true })
          this.pendingFocus.set(null)
        },
      },
      { injector: this.injector }
    )
  }

  /** Whether the list was drawn with days: the days drawn after that do not rise in. */
  private risen = false

  /** `(animate.enter)` of a day: the first days rise in, without a lasting fill. */
  protected rise(event: AnimationCallbackEvent, index: number): void {
    if (this.risen || index >= RISING_DAYS) return
    this.playOnce(event.target, 'day-rise', true)
  }

  private playOnce(element: Element, name: string, subtree = false): void {
    element.classList.add(name)
    const animations = element.getAnimations({ subtree })
    void Promise.allSettled(animations.map(animation => animation.finished)).then(() =>
      element.classList.remove(name)
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

  protected collectRange(range: DayRange): void {
    void this.data.collectRange(range)
  }

  protected focusFirstEntry(day: DayKey): void {
    const header = this.layout().indexes.get(day)
    if (header !== undefined) this.focusItem(header + 1)
  }

  protected selectEntry(entry: EntryUiAdapter): void {
    void this.viewState.openEvent(entry.trackKey)
  }
}
