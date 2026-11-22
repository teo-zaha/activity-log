import { type ElementRef, type Signal, signal } from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import { resizeObserver } from '@signality/core/observers/resize-observer'

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
 * document resets the scroller. Put back before the first paint, with a scroll event, so the
 * virtual list draws the days of where the scroller is.
 */
export function keepScroll(scroller: Signal<ElementRef<HTMLElement>>): KeptScroll {
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
    // Back in the document, before it is painted: where the list was left. Attached first, so
    // what follows the scroll event reads the list as back.
    attached.set(true)
    element.scrollTop = lastScrollTop
    element.dispatchEvent(new Event('scroll'))
    // AutoAnimate may have measured rows while they were out of the document; it measures a
    // list's rows again when an animation inside an element holding the list ends.
    element.dispatchEvent(new Event('animationend'))
  })
  listener.passive(scroller, 'scroll', () => {
    if (attached()) lastScrollTop = scroller().nativeElement.scrollTop
  })

  return { attached: attached.asReadonly() }
}
