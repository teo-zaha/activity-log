import { Component, signal, viewChild, type ElementRef } from '@angular/core'
import { render } from '@testing-library/angular'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import { stubLayoutObservers } from '../../testing/observers'
import type { DayKey } from '../../utils/local-day'
import { slideLine, trackActiveDay, type DayPositions, type ScrollMetrics } from './active-day'

describe('slideLine', () => {
  // A 600 px screen with the line 42 px down, over a list 2000 px tall: the last 558 px slide.
  const list: Omit<ScrollMetrics, 'scrollTop'> = { height: 600, line: 42, scrollHeight: 2000 }

  it('keeps the line in place at the top and until the last stretch', () => {
    expect(slideLine({ ...list, scrollTop: 0 })).toBe(0)
    expect(slideLine({ ...list, scrollTop: 842 })).toBe(0)
  })

  it('slides it to the bottom edge at the end of the list', () => {
    expect(slideLine({ ...list, scrollTop: 1400 })).toBe(557)
    expect(42 + slideLine({ ...list, scrollTop: 1400 })).toBe(list.height - 1)
  })

  it('slides it in proportion over the last stretch', () => {
    expect(slideLine({ ...list, scrollTop: 1400 - 279 })).toBeCloseTo(278.5)
  })

  it('stays at the bottom edge while the list bounces past its end', () => {
    expect(slideLine({ ...list, scrollTop: 1450 })).toBe(557)
  })

  it('slides over the whole overflow of a list barely taller than the screen', () => {
    const short = { ...list, scrollHeight: 700 }

    expect(slideLine({ ...short, scrollTop: 0 })).toBe(0)
    expect(slideLine({ ...short, scrollTop: 50 })).toBeCloseTo(278.5)
    expect(slideLine({ ...short, scrollTop: 100 })).toBe(557)
  })

  it('does not slide a list that does not scroll', () => {
    expect(slideLine({ ...list, scrollHeight: 600, scrollTop: 0 })).toBe(0)
  })
})

const DAYS: readonly DayKey[] = ['2026-05-18', '2026-05-17', '2026-05-16']
/** Each day 500 px tall, the list 40 px below the scroller's top. */
const DAY_HEIGHT = 500
const LIST_TOP = 40

/** A scroller over three days, placed as a virtual list would place them. */
@Component({ template: `<div #scroller></div>` })
class TrackedList {
  readonly attached = signal(true)
  readonly scrolledTo = vi.fn<(index: number) => void>()
  readonly positions: DayPositions = {
    indexAt: offset =>
      Math.min(DAYS.length - 1, Math.max(0, Math.floor((offset - LIST_TOP) / DAY_HEIGHT))),
    scrollTo: index => this.scrolledTo(index),
  }
  readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')
  readonly active = trackActiveDay({
    scroller: this.scroller,
    days: signal(DAYS),
    positions: this.positions,
    viewport: signal({
      height: 600,
      headerTop: 40,
      scrollHeight: LIST_TOP + DAYS.length * DAY_HEIGHT,
    }),
    enabled: signal(true),
    attached: this.attached,
  })
}

/** A scroller whose position sticks and whose list is 1540 px tall, as jsdom's is not. */
function scrollable(element: HTMLElement): (top: number) => void {
  let scrollTop = 0
  Object.defineProperty(element, 'scrollTop', { get: () => scrollTop })
  return top => {
    scrollTop = top
    element.dispatchEvent(new Event('scroll'))
  }
}

describe('trackActiveDay', () => {
  beforeEach(() => stubLayoutObservers())

  async function renderList(): Promise<{ list: TrackedList; scrollTo: (top: number) => void }> {
    const { fixture } = await render(TrackedList)
    const list = fixture.componentInstance
    return { list, scrollTo: scrollable(list.scroller().nativeElement) }
  }

  it('is the day under the line below the sticky header', async () => {
    const { list, scrollTo } = await renderList()
    expect(list.active.day()).toBe('2026-05-18')

    // The line runs 42 px below the top: at 500 it is 502 down, in the second day.
    scrollTo(500)
    expect(list.active.day()).toBe('2026-05-17')
  })

  it('makes the last day active at the end of the list, where the line slides down', async () => {
    const { list, scrollTo } = await renderList()

    scrollTo(LIST_TOP + DAYS.length * DAY_HEIGHT - 600)

    expect(list.active.day()).toBe('2026-05-16')
  })

  it('keeps the day while the list is out of the document, where its scroller reads empty', async () => {
    const { list, scrollTo } = await renderList()
    scrollTo(500)

    list.attached.set(false)
    scrollTo(0)
    expect(list.active.day()).toBe('2026-05-17')

    list.attached.set(true)
    scrollTo(0)
    expect(list.active.day()).toBe('2026-05-18')
  })

  it('scrolls to a jumped-to day and keeps it until the user scrolls', async () => {
    const { list, scrollTo } = await renderList()
    const scroller = list.scroller().nativeElement

    list.active.jumpTo('2026-05-17')
    expect(list.scrolledTo).toHaveBeenCalledWith(1)
    expect(list.active.day()).toBe('2026-05-17')

    // The jump's own scroll keeps it; a wheel releases it.
    scrollTo(0)
    expect(list.active.day()).toBe('2026-05-17')
    scroller.dispatchEvent(new WheelEvent('wheel'))
    scrollTo(0)
    expect(list.active.day()).toBe('2026-05-18')
  })

  it('jumps nowhere for a day the list does not show', async () => {
    const { list } = await renderList()

    list.active.jumpTo('2026-05-01')

    expect(list.scrolledTo).not.toHaveBeenCalled()
    expect(list.active.day()).toBe('2026-05-18')
  })
})
