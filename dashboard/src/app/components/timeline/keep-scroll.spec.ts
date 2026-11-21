import { Component, ElementRef, viewChild, viewChildren } from '@angular/core'
import { render } from '@testing-library/angular'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { anchoringShift, keepScroll, type ResizedSection } from './keep-scroll'

const at = (target: Element, height: number, bottom: number): ResizedSection => ({
  target,
  height,
  bottom: () => bottom,
})

describe('anchoringShift', () => {
  const [above, below] = [document.createElement('section'), document.createElement('section')]

  it('moves nothing for sections seen for the first time', () => {
    const heights = new WeakMap<Element, number>()

    expect(anchoringShift([at(above, 400, 100), at(below, 400, 900)], heights, 200)).toBe(0)
    expect(heights.get(above)).toBe(400)
  })

  it('takes up what the sections above the line grew or shrank by', () => {
    const heights = new WeakMap<Element, number>([
      [above, 400],
      [below, 400],
    ])

    expect(anchoringShift([at(above, 520, 100), at(below, 460, 900)], heights, 200)).toBe(120)
    expect(anchoringShift([at(above, 500, 100)], heights, 200)).toBe(-20)
    expect(heights.get(below)).toBe(460)
  })
})

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
  template: `<div #scroller class="scroller">
    <section #section data-day="2026-05-18"></section>
    <section #section data-day="2026-05-17"></section>
  </div>`,
})
class ScrollHost {
  readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')
  readonly sections = viewChildren<ElementRef<HTMLElement>>('section')
  readonly kept = keepScroll({
    scroller: this.scroller,
    sections: this.sections,
    headerTop: () => 40,
  })
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

  async function renderHost(
    anchoring: boolean
  ): Promise<{ host: ScrollHost; element: HTMLElement }> {
    vi.stubGlobal('ResizeObserver', FakeResizeObserver)
    vi.stubGlobal('CSS', { supports: () => anchoring })
    const { fixture } = await render(ScrollHost)
    await fixture.whenStable()
    const element = fixture.componentInstance.scroller().nativeElement
    scrollable(element)
    return { host: fixture.componentInstance, element }
  }

  it('puts the scroll position back when the list comes back into the document', async () => {
    const { host, element } = await renderHost(true)
    const observer = FakeResizeObserver.observing(element)
    const parent = element.parentElement!
    const animationEnd = vi.fn()
    element.addEventListener('animationend', animationEnd)
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
  })

  it('leaves a list in the document where it is', async () => {
    const { host, element } = await renderHost(true)
    element.scrollTop = 300

    FakeResizeObserver.observing(element).report()

    expect(element.scrollTop).toBe(300)
    expect(host.kept.attached()).toBe(true)
  })

  it('without scroll anchoring, keeps the list still as a day above the screen grows', async () => {
    const { host, element } = await renderHost(false)
    const [above, below] = host.sections().map(section => section.nativeElement)
    vi.spyOn(element, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 100, 600))
    vi.spyOn(above!, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, -500, 100, 420))
    vi.spyOn(below!, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, -80, 100, 400))
    const sections = FakeResizeObserver.observing(above!)
    sections.report(
      new Map([
        [above!, 300],
        [below!, 400],
      ])
    )
    element.scrollTop = 1000

    sections.report(
      new Map([
        [above!, 420],
        [below!, 400],
      ])
    )

    expect(element.scrollTop).toBe(1120)
  })

  it('leaves the scrolling to a browser that anchors it', async () => {
    const { host } = await renderHost(true)
    const [above] = host.sections().map(section => section.nativeElement)

    expect(FakeResizeObserver.all.some(observer => observer.targets.has(above!))).toBe(false)
  })
})
