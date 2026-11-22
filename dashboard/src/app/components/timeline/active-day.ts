import {
  type ElementRef,
  type Signal,
  afterRenderEffect,
  computed,
  signal,
  untracked,
} from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import type { DayKey } from '../../utils/local-day'

/** How far below the sticky day header the line that picks the active day runs. */
const TRACK_LINE_GAP = 2

/** What the line that picks the active day is measured against. */
export interface ScrollMetrics {
  /** The scroller's visible height. */
  readonly height: number
  /** How far below the scroller's top the line runs. */
  readonly line: number
  readonly scrollHeight: number
  readonly scrollTop: number
}

/**
 * How far below its place the line runs. The last days can never scroll up to the line: over the
 * last stretch of scrolling, as long as the screen below the line, it slides down to the bottom
 * edge, so each of them becomes active in turn and the last one at the end of the list.
 */
export function slideLine({ height, line, scrollHeight, scrollTop }: ScrollMetrics): number {
  const zone = Math.min(height - line, scrollHeight - height)
  const remaining = scrollHeight - height - scrollTop
  if (zone <= 0 || remaining >= zone) return 0
  return (height - 1 - line) * (1 - Math.max(0, remaining) / zone)
}

/** Where the days lie in the scroller, as the virtual list has them measured or estimated. */
export interface DayPositions {
  /** The index of the day at `offset` from the top of the scroller's content; null without days. */
  indexAt(offset: number): number | null
  /** Scrolls the day of `index` to the sticky header line, correcting as days are measured. */
  scrollTo(index: number): void
}

export interface ActiveDayOptions {
  readonly scroller: Signal<ElementRef<HTMLElement>>
  /** The days the list shows, top to bottom. */
  readonly days: Signal<readonly DayKey[]>
  readonly positions: DayPositions
  /** The scroller's height and where the sticky day header sits below its top. */
  readonly viewport: Signal<{ readonly height: number; readonly headerTop: number }>
  /** Whether the day is tracked: only while the day index shows. */
  readonly enabled: Signal<boolean>
  /** False while the list is out of the document: its scroll position means nothing then. */
  readonly attached: Signal<boolean>
}

export interface ActiveDay {
  /** The day whose header is stuck at the top, else the first day; null without days. */
  readonly day: Signal<DayKey | null>
  /** Lands the day on the sticky header line and keeps it active until the user scrolls. */
  jumpTo(day: DayKey): void
}

/**
 * The day the list is at: the one under a line just below the sticky day header, sliding over
 * the last stretch (`slideLine`), or the day jumped to until the user scrolls. Read from where
 * the virtual list places the days, so a fast scroll that skips days, or a day not drawn, still
 * gives the day under the line.
 */
export function trackActiveDay(options: ActiveDayOptions): ActiveDay {
  const { scroller, days, positions, viewport, enabled, attached } = options
  /** The day as last read under the line or set by a jump. */
  const tracked = signal<DayKey | null>(null)
  /** After a jump the chosen day stays active (the last days may never reach the header line). */
  let pinned = false

  const track = (): void => {
    if (pinned || !enabled() || !attached()) return
    const element = scroller().nativeElement
    const line = viewport().headerTop + TRACK_LINE_GAP
    const slide = slideLine({
      height: element.clientHeight,
      line,
      scrollHeight: element.scrollHeight,
      scrollTop: element.scrollTop,
    })
    const index = positions.indexAt(element.scrollTop + line + slide)
    const day = index === null ? undefined : days()[index]
    if (day !== undefined) tracked.set(day)
  }

  // Any scroll the user starts releases a jumped-to day; presses and keys on the rail do not.
  const release = (event: Event): void => {
    const fromRail = event.target instanceof Element && event.target.closest('app-day-index')
    if (event.type === 'wheel' || !fromRail) pinned = false
  }
  for (const type of ['wheel', 'touchstart', 'keydown', 'pointerdown']) {
    listener.passive(scroller, type, release)
  }
  listener.passive(scroller, 'scroll', track)
  // The list or its size changed without a scroll (a filter, a collection, a resize), or the
  // list is back in the document.
  afterRenderEffect(() => {
    days()
    viewport()
    if (enabled() && attached()) untracked(track)
  })

  return {
    day: computed(() => {
      const shown = days()
      const day = tracked()
      return shown.find(candidate => candidate === day) ?? shown[0] ?? null
    }),
    jumpTo(day): void {
      const index = days().indexOf(day)
      if (index < 0) return
      pinned = true
      tracked.set(day)
      positions.scrollTo(index)
    },
  }
}
