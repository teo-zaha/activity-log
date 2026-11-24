import { type ElementRef, type Signal, signal } from '@angular/core'
import { resizeObserver } from '@signality/core/observers/resize-observer'

/**
 * Whether the scroller is in the document: the router keeps the timeline out of it while a
 * calendar view shows (`TimelineReuseStrategy`). False from leaving it until the scroller is back.
 *
 * Back in the document the scroller starts at the top, without a scroll event: one is sent, so
 * the virtual list draws the days of where the scroller is, and the router then moves it to
 * where the history entry left it, or to the top (`ViewsScroller`).
 */
export function trackAttachment(scroller: Signal<ElementRef<HTMLElement>>): Signal<boolean> {
  const attached = signal(true)
  // A scroller that leaves the document shrinks to nothing, and one that comes back grows: both
  // are seen here, before the next paint.
  resizeObserver(scroller, () => {
    const element = scroller().nativeElement
    if (!element.isConnected) {
      attached.set(false)
      return
    }
    if (attached()) return
    // Attached first, so what follows the scroll event reads the list as back.
    attached.set(true)
    element.dispatchEvent(new Event('scroll'))
  })
  return attached.asReadonly()
}
