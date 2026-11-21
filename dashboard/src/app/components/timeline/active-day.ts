import {
  DestroyRef,
  type ElementRef,
  type Signal,
  afterRenderEffect,
  computed,
  inject,
  signal,
} from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import type { DayKey } from '../../utils/local-day'

/** How many frames after a jump its landing is corrected, see `jumpTo`. */
const ALIGN_FRAMES = 4
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

/**
 * The index of the day at height `y`: the first of `count` sections, top to bottom, whose bottom
 * reaches below it. Above the first section that is the first day, past the last the last one;
 * -1 without sections. `bottomOf` is read for a few sections only, a binary search.
 */
export function dayAt(count: number, bottomOf: (index: number) => number, y: number): number {
  if (count === 0) return -1
  let low = 0
  let high = count - 1
  while (low < high) {
    const middle = (low + high) >> 1
    if (bottomOf(middle) > y) high = middle
    else low = middle + 1
  }
  return low
}

export interface ActiveDayOptions {
  readonly scroller: Signal<ElementRef<HTMLElement>>
  /** The days' sections, top to bottom, each with its day in `data-day`. */
  readonly sections: Signal<readonly ElementRef<HTMLElement>[]>
  /** The days the list shows, top to bottom. */
  readonly days: Signal<readonly DayKey[]>
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
  /** Lands the day's section on the sticky header line and keeps it active until the user scrolls. */
  jumpTo(day: string): void
  /** The section of a day the list shows. */
  sectionOf(day: string): HTMLElement | undefined
}

/**
 * The day the list is at: the section crossing a 1 px line just under the sticky day header,
 * sliding over the last stretch (`slideLine`), or the day jumped to until the user scrolls.
 */
export function trackActiveDay(options: ActiveDayOptions): ActiveDay {
  const { scroller, sections, viewport, enabled, attached } = options
  /** The day as last seen by the observer or set by a jump. */
  const tracked = signal<string | null>(null)
  /** After a jump the chosen day stays active (the last days may never reach the header line). */
  let pinned = false
  /** The frame that next re-aligns a jumped-to day, see `jumpTo`. */
  let alignFrame = 0

  /** The sections by day, for jumps: they look a day up on every frame they correct. */
  const sectionsByDay = computed(
    () =>
      new Map(
        sections().map(({ nativeElement }) => [
          nativeElement.getAttribute('data-day') ?? '',
          nativeElement,
        ])
      )
  )
  const sectionOf = (day: string): HTMLElement | undefined => sectionsByDay().get(day)

  const track = (): void => {
    if (pinned || !enabled() || !attached()) return
    const element = scroller().nativeElement
    const height = element.clientHeight
    const line = viewport().headerTop + TRACK_LINE_GAP
    const slide = slideLine({
      height,
      line,
      scrollHeight: element.scrollHeight,
      scrollTop: element.scrollTop,
    })
    const list = sections()
    const index = dayAt(
      list.length,
      i => list[i]?.nativeElement.getBoundingClientRect().bottom ?? 0,
      element.getBoundingClientRect().top + line + slide
    )
    const day = list[index]?.nativeElement.getAttribute('data-day')
    if (day) tracked.set(day)
  }

  const alignTo = (day: string, frames: number): void => {
    const section = sectionOf(day)
    if (!section || !pinned) return
    const element = scroller().nativeElement
    const offset =
      section.getBoundingClientRect().top -
      element.getBoundingClientRect().top -
      viewport().headerTop
    if (Math.abs(offset) >= 1) {
      const before = element.scrollTop
      element.scrollTop += offset
      // At either end of the list the day cannot reach the line: nothing left to correct.
      if (element.scrollTop === before) return
    }
    // Hand-written: Signality has no animation-frame scheduling.
    if (frames > 0) alignFrame = requestAnimationFrame(() => alignTo(day, frames - 1))
  }

  // Any scroll the user starts releases a jumped-to day; presses and keys on the rail do not.
  const release = (event: Event): void => {
    const fromRail = event.target instanceof Element && event.target.closest('app-day-index')
    if (event.type === 'wheel' || !fromRail) pinned = false
  }
  for (const type of ['wheel', 'touchstart', 'keydown', 'pointerdown']) {
    listener.passive(scroller, type, release)
  }
  // Near the end the line slides, which no section crossing tells the observer below.
  listener.passive(scroller, 'scroll', track)
  inject(DestroyRef).onDestroy(() => cancelAnimationFrame(alignFrame))

  // The observer only says when a section crosses the line; the day is then read from where the
  // sections are, since a fast scroll can skip days and leave the line where none is (the list's
  // top padding). Hand-written: Signality's `intersectionObserver` takes a fixed list of targets,
  // and these sections change with the list and are observed only while the day index shows.
  afterRenderEffect(onCleanup => {
    const observed = sections()
    const { height, headerTop } = viewport()
    if (!enabled() || height === 0) return
    const line = headerTop + TRACK_LINE_GAP
    const observer = new IntersectionObserver(track, {
      root: scroller().nativeElement,
      rootMargin: `-${line}px 0px -${Math.max(0, height - line - 1)}px 0px`,
    })
    for (const section of observed) observer.observe(section.nativeElement)
    onCleanup(() => observer.disconnect())
  })

  return {
    day: computed(() => {
      const days = options.days()
      const day = tracked()
      return days.find(shown => shown === day) ?? days[0] ?? null
    }),
    jumpTo(day): void {
      if (!sectionOf(day)) return
      pinned = true
      tracked.set(day)
      cancelAnimationFrame(alignFrame)
      alignTo(day, ALIGN_FRAMES)
    },
    sectionOf,
  }
}
