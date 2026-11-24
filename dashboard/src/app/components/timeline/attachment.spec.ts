import { Component, ElementRef, viewChild } from '@angular/core'
import { render } from '@testing-library/angular'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { trackAttachment } from './attachment'

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

@Component({ template: `<div #scroller class="scroller"></div>` })
class ScrollHost {
  readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')
  readonly attached = trackAttachment(this.scroller)
}

describe('trackAttachment', () => {
  afterEach(() => {
    FakeResizeObserver.all.length = 0
    vi.unstubAllGlobals()
  })

  async function renderHost(): Promise<{ host: ScrollHost; element: HTMLElement }> {
    vi.stubGlobal('ResizeObserver', FakeResizeObserver)
    const { fixture } = await render(ScrollHost)
    await fixture.whenStable()
    return {
      host: fixture.componentInstance,
      element: fixture.componentInstance.scroller().nativeElement,
    }
  }

  it('follows the scroller out of the document and back', async () => {
    const { host, element } = await renderHost()
    const observer = FakeResizeObserver.observing(element)
    const parent = element.parentElement!

    element.remove()
    observer.report()
    expect(host.attached()).toBe(false)

    parent.append(element)
    observer.report()
    expect(host.attached()).toBe(true)
  })

  it('has the list read where the scroller is once it is back', async () => {
    const { element } = await renderHost()
    const observer = FakeResizeObserver.observing(element)
    const parent = element.parentElement!
    const scroll = vi.fn()
    element.addEventListener('scroll', scroll)

    element.remove()
    observer.report()
    parent.append(element)
    observer.report()

    expect(scroll).toHaveBeenCalledOnce()
  })

  it('sends nothing while the scroller stays in the document', async () => {
    const { element } = await renderHost()
    const scroll = vi.fn()
    element.addEventListener('scroll', scroll)

    FakeResizeObserver.observing(element).report()

    expect(scroll).not.toHaveBeenCalled()
  })
})
