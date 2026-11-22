import { Component, ElementRef, viewChild } from '@angular/core'
import { render } from '@testing-library/angular'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { keepScroll } from './keep-scroll'

/** A `ResizeObserver` that reports only when a spec says so. */
class FakeResizeObserver {
  static readonly all: FakeResizeObserver[] = []
  readonly targets = new Set<Element>()
  constructor(private readonly callback: ResizeObserverCallback) {
    FakeResizeObserver.all.push(this)
  }
  observe(target: Element): void {
    this.targets.add(target)
  }
  unobserve(target: Element): void {
    this.targets.delete(target)
  }
  disconnect(): void {
    this.targets.clear()
  }
  /** Reports the targets it observes, at the given border-box heights. */
  report(heights: ReadonlyMap<Element, number> = new Map()): void {
    const entries = [...this.targets].map((target): ResizeObserverEntry => {
      const height = heights.get(target) ?? 0
      const size = [{ blockSize: height, inlineSize: 100 }]
      return {
        target,
        borderBoxSize: size,
        contentBoxSize: size,
        devicePixelContentBoxSize: size,
        contentRect: new DOMRect(0, 0, 100, height),
      }
    })
    this.callback(entries, this)
  }
  static observing(target: Element): FakeResizeObserver {
    const found = FakeResizeObserver.all.find(observer => observer.targets.has(target))
    if (!found) throw new Error('nothing observes the target')
    return found
  }
}

@Component({
  template: `<div #scroller class="scroller"></div>`,
})
class ScrollHost {
  readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')
  readonly kept = keepScroll(this.scroller)
}

/** A scroller whose position sticks, as jsdom's does not. */
function scrollable(element: HTMLElement): void {
  let top = 0
  Object.defineProperty(element, 'scrollTop', {
    get: () => top,
    set: (value: number) => (top = value),
  })
}

describe('keepScroll', () => {
  afterEach(() => {
    FakeResizeObserver.all.length = 0
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
  })

  async function renderHost(): Promise<{ host: ScrollHost; element: HTMLElement }> {
    vi.stubGlobal('ResizeObserver', FakeResizeObserver)
    const { fixture } = await render(ScrollHost)
    await fixture.whenStable()
    const element = fixture.componentInstance.scroller().nativeElement
    scrollable(element)
    return { host: fixture.componentInstance, element }
  }

  it('puts the scroll position back when the list comes back into the document', async () => {
    const { host, element } = await renderHost()
    const observer = FakeResizeObserver.observing(element)
    const parent = element.parentElement!
    const animationEnd = vi.fn()
    element.addEventListener('animationend', animationEnd)
    const scrolledTo: number[] = []
    element.addEventListener('scroll', () => scrolledTo.push(element.scrollTop))
    element.scrollTop = 300
    element.dispatchEvent(new Event('scroll'))

    element.remove()
    observer.report()
    expect(host.kept.attached()).toBe(false)
    // Out of the document the scroller is reset, and what it reports then is not kept.
    element.scrollTop = 0
    element.dispatchEvent(new Event('scroll'))

    parent.append(element)
    observer.report()
    expect(element.scrollTop).toBe(300)
    expect(host.kept.attached()).toBe(true)
    expect(animationEnd).toHaveBeenCalledOnce()
    // The virtual list hears where the scroller is.
    expect(scrolledTo).toEqual([300, 0, 300])
  })

  it('leaves a list in the document where it is', async () => {
    const { host, element } = await renderHost()
    element.scrollTop = 300

    FakeResizeObserver.observing(element).report()

    expect(element.scrollTop).toBe(300)
    expect(host.kept.attached()).toBe(true)
  })
})
