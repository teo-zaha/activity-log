import { type ElementRef, type Signal, afterRenderEffect, untracked } from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import type { ScrollPlaces } from '../../services/views-scroller.service'
import type { TimelineItems } from './timeline-items'

interface Coordinates {
  readonly startOf: (index: number) => number | undefined
  readonly indexAt: (offset: number) => number | undefined
  readonly maxScroll: number
  readonly line: number
}

interface Anchor {
  readonly key: string
  readonly contentY: number
  readonly scrollTop: number
}

interface KeepPlaceOptions {
  readonly scroller: Signal<ElementRef<HTMLElement>>
  readonly layout: Signal<TimelineItems>
  readonly coordinates: () => Coordinates
}

/** A removed row hands over its offset to the next row that survives, then to its day. */
function heirOf(key: string, before: TimelineItems, after: TimelineItems): string | undefined {
  if (after.indexes.has(key)) return key
  const index = before.indexes.get(key)
  if (index === undefined || before.items[index]?.kind !== 'row') return undefined
  for (let next = index + 1; next < before.items.length; next++) {
    const item = before.items[next]!
    if (item.kind === 'row' && after.indexes.has(item.key)) return item.key
  }
  const day = before.days[before.items[index].dayIndex]!.day.date
  return after.indexes.has(day) ? day : undefined
}

function anchorAt(
  scrollTop: number,
  layout: TimelineItems,
  coordinates: Coordinates
): Anchor | null {
  if (scrollTop <= 0) return null
  const line = scrollTop + coordinates.line
  const index = coordinates.indexAt(line)
  if (index === undefined) return null
  for (let next = index; next < layout.items.length; next++) {
    const item = layout.items[next]!
    const contentY = coordinates.startOf(next)
    if (item.kind === 'row' && contentY !== undefined && contentY >= line) {
      return { key: item.key, contentY, scrollTop }
    }
  }
  const day = layout.days[layout.items[index]!.dayIndex]!
  const contentY = coordinates.startOf(day.header)
  return contentY === undefined ? null : { key: day.day.date, contentY, scrollTop }
}

/**
 * Uses the same positions as the rendered rows. ResizeObserver supplies the sizes after layout
 * and Angular writes the row offsets and scroll correction together. In particular, keeping the
 * place after a render never asks the browser to synchronously lay out the new rows.
 */
export function keepPlace({ scroller, layout, coordinates }: KeepPlaceOptions): ScrollPlaces {
  let anchor: Anchor | null = null
  let readAmong: TimelineItems | undefined
  /** The last offset this wrote, until a scroll event reports a different one. */
  let written: number | null = null
  const root = (): HTMLElement => scroller().nativeElement

  function read(scrollTop: number): Anchor | null {
    readAmong = layout()
    return anchorAt(scrollTop, readAmong, coordinates())
  }

  function scrollTo(now: number, target: number, maxScroll: number): number {
    const top = Math.round(Math.max(0, Math.min(target, maxScroll)))
    if (Math.abs(top - now) >= 0.5) {
      written = top
      root().scrollTop = top
    }
    return top
  }

  listener.passive(scroller, 'scroll', () => {
    const element = root()
    if (!element.isConnected) return
    const top = element.scrollTop
    if (written !== null && Math.abs(top - written) < 1) return
    if (anchor && readAmong !== untracked(layout)) {
      // Keep the user's scroll if a new list is waiting to render. Moving to the new end can
      // come from the browser clamping a shorter list, so preserve the earlier offset there.
      if (top < untracked(coordinates).maxScroll - 1) {
        written = null
        anchor = top <= 0 ? null : { ...anchor, scrollTop: top }
      }
      return
    }
    written = null
    anchor = untracked(() => read(top))
  })

  afterRenderEffect({
    write: () => {
      const next = layout()
      const current = coordinates()
      untracked(() => {
        if (!root().isConnected) {
          anchor = null
          return
        }
        const held = anchor
        if (!held || !readAmong) return
        const key = heirOf(held.key, readAmong, next)
        readAmong = next
        const index = key === undefined ? undefined : next.indexes.get(key)
        const contentY = index === undefined ? undefined : current.startOf(index)
        if (key === undefined || contentY === undefined) {
          anchor = null
          return
        }
        const scrollTop = scrollTo(
          held.scrollTop,
          held.scrollTop + contentY - held.contentY,
          current.maxScroll
        )
        anchor = scrollTop === 0 ? null : { key, contentY, scrollTop }
      })
    },
  })

  return {
    save: (): (() => boolean) | null => {
      const saved = anchorAt(root().scrollTop, layout(), coordinates())
      if (saved === null) return null
      return () => {
        const next = layout()
        const index = next.indexes.get(saved.key)
        if (index === undefined) return false
        const current = coordinates()
        const contentY = current.startOf(index)
        if (contentY === undefined) return false
        // On reattachment the browser starts at its own offset. Router restoration runs outside
        // the render correction and can read it here, before writing the saved position.
        const scrollTop = scrollTo(
          root().scrollTop,
          saved.scrollTop + contentY - saved.contentY,
          current.maxScroll
        )
        anchor = scrollTop === 0 ? null : { key: saved.key, contentY, scrollTop }
        readAmong = next
        return true
      }
    },
  }
}
