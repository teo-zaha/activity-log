import { type ElementRef, type Signal, afterRenderEffect, untracked } from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import type { DayKey } from '../../utils/local-day'
import type { DayPositions } from './active-day'

/** Where the virtual list places the days, and how it is scrolled to a place. */
export interface PlacePositions extends Pick<DayPositions, 'indexAt'> {
  /** How far down the scroller's content the day of `index` starts; null without it. */
  startOf(index: number): number | null
  scrollToOffset(offset: number): void
}

export interface KeepPlaceOptions {
  readonly scroller: Signal<ElementRef<HTMLElement>>
  /** The days the list shows, top to bottom. */
  readonly days: Signal<readonly DayKey[]>
  readonly positions: PlacePositions
  /** How far below the scroller's top the reader reads: just under the sticky day header. */
  readonly line: () => number
  /** False while the list is out of the document: its scroll position means nothing then. */
  readonly attached: Signal<boolean>
}

/** The day under the line, and how far down it the line runs. */
interface Place {
  readonly day: DayKey
  readonly into: number
}

/**
 * Keeps the reader's place as the days change (a collection, a filter): the day under the line
 * below the sticky header stays where it was, whatever the days above it became. Those days are
 * new items of the virtual list, estimated until they are drawn, and neither the virtual list
 * (which keeps a place only at the list's end) nor the browser (whose anchoring the virtual list
 * turns off) would keep it. At the top of the list nothing is kept, so the newest days a
 * collection brings come into view.
 */
export function keepPlace(options: KeepPlaceOptions): void {
  const { scroller, days, positions, line, attached } = options
  /** Where the reader was as the list last scrolled; null at the top. */
  let place: Place | null = null

  listener.passive(scroller, 'scroll', () => {
    if (!attached()) return
    const top = scroller().nativeElement.scrollTop
    if (top <= 0) {
      place = null
      return
    }
    const offset = top + line()
    const index = positions.indexAt(offset)
    const day = index === null ? undefined : untracked(days)[index]
    const start = index === null ? null : positions.startOf(index)
    place = day === undefined || start === null ? null : { day, into: offset - start }
  })

  // After the render that drew the changed days: the day read back under the line.
  afterRenderEffect(() => {
    const shown = days()
    untracked(() => {
      if (place === null || !attached()) return
      const index = shown.indexOf(place.day)
      const start = index < 0 ? null : positions.startOf(index)
      if (start === null) return
      const target = start + place.into - line()
      if (Math.abs(target - scroller().nativeElement.scrollTop) >= 1) {
        positions.scrollToOffset(target)
      }
    })
  })
}
