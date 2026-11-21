import { type ElementRef, type Signal, afterRenderEffect, signal } from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import { resizeObserver } from '@signality/core/observers/resize-observer'

/** A section that changed size, as a `ResizeObserver` reports it and where it is now. */
export interface ResizedSection {
  readonly target: Element
  readonly height: number
  /** Its bottom edge, in viewport coordinates; read only for a section seen before. */
  readonly bottom: () => number
}

/**
 * How far the scroll position must move so the list does not move under the reader: the sum of
 * what the sections wholly above `line` grew by since they were last seen. Records each section's
 * height in `heights`; a section seen for the first time moves nothing.
 */
export function anchoringShift(
  resized: readonly ResizedSection[],
  heights: WeakMap<Element, number>,
  line: number
): number {
  let shift = 0
  for (const { target, height, bottom } of resized) {
    const before = heights.get(target)
    heights.set(target, height)
    if (before !== undefined && bottom() <= line) shift += height - before
  }
  return shift
}

export interface KeepScrollOptions {
  readonly scroller: Signal<ElementRef<HTMLElement>>
  /** The sections whose late heights could move the list under the reader. */
  readonly sections: Signal<readonly ElementRef<HTMLElement>[]>
  /** Where the sticky day header sits below the scroller's top: what the reader reads under. */
  readonly headerTop: () => number
}

export interface KeptScroll {
  /**
   * False from leaving the document until the scroller is back and its position put back. Lists
   * do not animate meanwhile: AutoAnimate would measure their rows out of the document (as 0 × 0
   * at the corner) and fly them in from there, so it starts afresh once the list is back.
   */
  readonly attached: Signal<boolean>
}

/**
 * Keeps a scroller's position while its element leaves the document and comes back: the router
 * keeps the timeline while a calendar view shows (`TimelineReuseStrategy`), but leaving the
 * document resets the scroller. Where the browser does not anchor scrolling (WebKit), also keeps
 * the list still while sections above the screen take their real heights.
 */
export function keepScroll(options: KeepScrollOptions): KeptScroll {
  const { scroller } = options
  const attached = signal(true)
  /** Where the list was scrolled to, while it was in the document. */
  let lastScrollTop = 0

  // A scroller that leaves the document shrinks to nothing, and one that comes back grows: both
  // are seen here, before the next paint.
  resizeObserver(scroller, () => {
    const element = scroller().nativeElement
    if (!element.isConnected) {
      attached.set(false)
      return
    }
    if (attached()) return
    // Back in the document, before it is painted: where the list was left.
    element.scrollTop = lastScrollTop
    // AutoAnimate may have measured rows while they were out of the document; it measures a
    // list's rows again when an animation inside an element holding the list ends.
    element.dispatchEvent(new Event('animationend'))
    attached.set(true)
  })
  listener.passive(scroller, 'scroll', () => {
    if (attached()) lastScrollTop = scroller().nativeElement.scrollTop
  })

  // Without scroll anchoring, a day above the screen that is drawn for the first time, its
  // estimated height turning real, would move the list under the reader: the scroll position
  // takes up the difference instead. Hand-written: the sections change with the list, and
  // Signality's `resizeObserver` takes a fixed list of targets.
  if (typeof CSS !== 'undefined' && !CSS.supports('overflow-anchor', 'auto')) {
    const heights = new WeakMap<Element, number>()
    afterRenderEffect(onCleanup => {
      const sections = options.sections()
      const element = scroller().nativeElement
      const observer = new ResizeObserver(entries => {
        if (!element.isConnected) return
        const shift = anchoringShift(
          entries.map(entry => ({
            target: entry.target,
            height: entry.borderBoxSize[0]?.blockSize ?? entry.contentRect.height,
            bottom: (): number => entry.target.getBoundingClientRect().bottom,
          })),
          heights,
          element.getBoundingClientRect().top + options.headerTop()
        )
        if (shift) element.scrollTop += shift
      })
      for (const section of sections) observer.observe(section.nativeElement, { box: 'border-box' })
      onCleanup(() => observer.disconnect())
    })
  }

  return { attached: attached.asReadonly() }
}
