import type { ListMotionFrame, PositionedItem } from './list-motion'

interface KeyedElement {
  readonly key: string
  readonly element: HTMLElement
  /** Its containing block stays put while the viewport scrolls, as in a sticky header. */
  readonly pinned?: boolean
}

/** Read absolute boxes relative to the scroller. Offsets ignore ongoing transforms on the items. */
export function measurePositionedElements(
  scroller: HTMLElement,
  elements: Iterable<KeyedElement>
): Pick<ListMotionFrame, 'keys' | 'items' | 'viewport' | 'beforeScroll' | 'scrollTop'> {
  const scrollTop = scroller.scrollTop
  const bounds = scroller.getBoundingClientRect()
  const viewportTop = bounds.top + scroller.clientTop
  const viewportLeft = bounds.left + scroller.clientLeft
  const origins = new Map<HTMLElement, { top: number; left: number }>()
  const items = new Map<string, PositionedItem>()
  for (const { key, element, pinned } of elements) {
    const parent = element.offsetParent
    if (!(parent instanceof HTMLElement)) continue
    let origin = origins.get(parent)
    if (!origin) {
      const rect = parent.getBoundingClientRect()
      origin = {
        top: rect.top + parent.clientTop - parent.scrollTop - viewportTop,
        left: rect.left + parent.clientLeft - parent.scrollLeft - viewportLeft,
      }
      origins.set(parent, origin)
    }
    const top = origin.top + element.offsetTop
    items.set(key, {
      element,
      size: element.offsetHeight,
      topAt: offset => (pinned ? top : top + scrollTop - offset),
      across: { left: origin.left + element.offsetLeft, width: element.offsetWidth },
    })
  }
  return {
    keys: items,
    items,
    viewport: { top: 0, bottom: scroller.clientHeight },
    beforeScroll: scrollTop,
    scrollTop,
  }
}
