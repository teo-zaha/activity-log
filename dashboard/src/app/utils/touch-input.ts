import { listener, setupSync } from '@signality/core/browser/listener'

/**
 * On `<html>` while the last pointer pressed was a finger: `ui.hover` (`_dashboard-shared.scss`)
 * shows no hover then, also on a device whose mouse or touchpad makes it one that can hover.
 */
export const TOUCH_INPUT_CLASS = 'touch-input'

/**
 * Follows the pointer that acts on the page: a finger turns hover styles off until a mouse moves
 * again (a pen hovers, so it turns them back on). Call in an injection context.
 */
export function followTouchInput(document: Document): void {
  const root = document.documentElement
  // At once, not after the first render: the page is the target, and a finger may come first.
  setupSync(() => {
    listener.capture.passive(document, 'pointerdown', (event: PointerEvent) => {
      root.classList.toggle(TOUCH_INPUT_CLASS, event.pointerType === 'touch')
    })
    listener.capture.passive(document, 'pointermove', (event: PointerEvent) => {
      if (event.pointerType === 'mouse') root.classList.remove(TOUCH_INPUT_CLASS)
    })
    // iOS shows a tapped control's `:active` state only on a page that listens for touches, and
    // then at once; the listener itself has nothing to do.
    listener.passive(document, 'touchstart', () => undefined)
  })
}
