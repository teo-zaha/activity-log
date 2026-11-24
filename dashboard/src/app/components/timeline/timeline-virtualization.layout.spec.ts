/// <reference types="@vitest/browser-playwright" />
import { Location } from '@angular/common'
import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { cdp, page, userEvent } from 'vitest/browser'
import type { ActivityLog, GitEntry } from '../../generated/activity'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { MockBackendTransport } from '../../testing/activity-fixtures'
import { openApp, type OpenApp } from '../../testing/app'
import type { DayKey } from '../../utils/local-day'

const HEAVY: DayKey = '2026-05-17'
const DAYS: readonly DayKey[] = ['2026-05-18', HEAVY, '2026-05-16']
const RANGE = 'from=2026-05-16&to=2026-05-18'
const COUNT = 1_000
const HEAVY_DAY = `[data-day="${HEAVY}"]`
const LONG =
  'Rebuild the reporting and reconciliation jobs for the customer migration, with an additional validation check at startup'

function commit(day: DayKey, index: number, added = false): GitEntry {
  const [year, month, date] = day.split('-').map(Number)
  const id = `${day}-${added ? 'new-' : ''}${index}`
  return {
    source: 'git',
    connection: 'default',
    timestamp: new Date(year!, month! - 1, date!, added ? 18 : 17, 0, -index * 16).toISOString(),
    id,
    hash: id,
    message: index % 3 === 0 ? `TICKET-7 ${LONG} ${index}` : `Change ${index}`,
    author: 'Dev',
    repo: 'reporting',
  }
}

const heavyRows = Array.from({ length: COUNT }, (_, index) => commit(HEAVY, index))
const ordinary = DAYS.filter(day => day !== HEAVY).map(day =>
  Array.from({ length: 6 }, (_, index) => commit(day, index))
)

function log(extra = 0, count = COUNT): ActivityLog {
  const added = Array.from({ length: extra }, (_, index) => commit(HEAVY, index, true))
  const entries = [...ordinary[0]!, ...added, ...heavyRows.slice(0, count), ...ordinary[1]!]
  return {
    meta: {
      generated: `2026-05-18T20:${String(extra).padStart(2, '0')}:00Z`,
      start: new Date(2026, 4, 16).toISOString(),
      end: new Date(2026, 4, 19).toISOString(),
    },
    summary: {
      connections: [{ connection: { source: 'git', name: 'default' }, count: entries.length }],
      mergedCount: 0,
    },
    entries,
  }
}

const keyOf = (entry: GitEntry): string => `git/${entry.connection}/${entry.id}`
const scroller = (): HTMLElement =>
  document.querySelector<HTMLElement>('app-timeline .timeline-scroll')!
const section = (): HTMLElement => scroller().querySelector<HTMLElement>(HEAVY_DAY)!
const entry = (key: string): HTMLElement | null =>
  scroller().querySelector(`.entry[data-entry="${key}"]`)
const drawn = (): number => scroller().querySelectorAll(HEAVY_DAY + ' .entry').length
const line = (): number =>
  parseFloat(getComputedStyle(scroller()).getPropertyValue('--day-header-top'))
const topOf = (element: HTMLElement): number =>
  element.getBoundingClientRect().top - scroller().getBoundingClientRect().top

async function frames(count = 3): Promise<void> {
  for (let i = 0; i < count; i++) {
    await new Promise(resolve => requestAnimationFrame(() => setTimeout(resolve)))
  }
}

async function open(count = COUNT, width = 1280): Promise<OpenApp> {
  await page.viewport(width, 800)
  const backend = new MockBackendTransport()
  backend.activityResponse = log(0, count)
  backend.activityResponses = [log(0, count)]
  const app = await openApp(`/timeline?${RANGE}`, { backend })
  await document.fonts.ready
  await expect.poll(() => drawn()).toBeGreaterThan(0)
  await expect.poll(() => scroller().querySelector('.day-rise')).toBeNull()
  return app
}

async function scroll(top: number): Promise<void> {
  scroller().scrollTop = top
  await frames()
}

function expectWindow(): void {
  expect(drawn()).toBeGreaterThan(0)
  expect(drawn()).toBeLessThanOrEqual(128)
}

function reading(): { key: string; top: number } {
  const row = [...section().querySelectorAll<HTMLElement>('.entry')].find(
    candidate => topOf(candidate) >= line()
  )
  if (!row) throw new Error('No visible reading row')
  return { key: row.dataset['entry']!, top: topOf(row) }
}

async function expectPlace(key: string, top: number): Promise<void> {
  await expect
    .poll(() => {
      const row = entry(key)
      return row ? Math.abs(topOf(row) - top) : Infinity
    })
    .toBeLessThanOrEqual(1)
  expectWindow()
}

async function collect(backend: MockBackendTransport, extra: number, count = COUNT): Promise<void> {
  backend.activityResponse = log(extra, count)
  backend.publish()
  await frames()
}

describe('virtualized rows in a 1,000-entry day', () => {
  it('keeps one bounded window, the pinned row and the sticky feed article', async () => {
    const { backend } = await open()
    const first = entry(keyOf(heavyRows[0]!))!
    first.focus({ preventScroll: true })
    expectWindow()

    for (const offset of [18_000, 43_000, 64_000]) {
      await scroll(offset)
      expectWindow()
      expect(document.activeElement).toBe(first)
      expect(first.isConnected).toBe(true)
      expect(section().getAttribute('aria-posinset')).toBe('2')
      expect(section().getAttribute('aria-setsize')).toBe('3')
      expect(topOf(section().querySelector('header')!)).toBeCloseTo(line(), 0)
      const near = section().querySelector('li:nth-child(2)')!
      expect(Number(near.getAttribute('aria-posinset'))).toBeGreaterThan(128)
      expect(near.getAttribute('aria-setsize')).toBe(String(COUNT))
    }

    await collect(backend, 40)
    await expect.poll(() => section().querySelector('.day-count')?.textContent).toBe('1040 events')
    await scroll(scroller().scrollHeight)
    expectWindow()
    expect(document.activeElement).toBe(first)
  })

  it('retains offscreen focus across a focusout with no destination', async () => {
    await open()
    const first = entry(keyOf(heavyRows[0]!))!
    first.focus({ preventScroll: true })
    await scroll(34_000)
    first.dispatchEvent(new FocusEvent('focusout', { bubbles: true }))
    await frames()
    expect(first.isConnected).toBe(true)

    scroller().querySelector('input')!.focus({ preventScroll: true })
    await expect.poll(() => first.isConnected).toBe(false)
  })

  it('wheels upward through unmeasured phone rows without repeated scroll corrections', async () => {
    await open(COUNT, 402)
    await scroll(34_000)
    const root = scroller()
    const start = root.scrollTop
    const { left, top, width, height } = root.getBoundingClientRect()
    const scrollTop = Object.getOwnPropertyDescriptor(Element.prototype, 'scrollTop')!
    const writes: number[] = []
    Object.defineProperty(root, 'scrollTop', {
      configurable: true,
      get: (): number => scrollTop.get!.call(root),
      set: (value: number): void => {
        writes.push(value)
        scrollTop.set!.call(root, value)
      },
    })

    try {
      for (let i = 0; i < 40; i++) {
        await cdp().send('Input.dispatchMouseEvent', {
          type: 'mouseWheel',
          x: left + width / 2,
          y: top + height / 2,
          deltaX: 0,
          deltaY: -142,
        })
        await frames(1)
      }
      await frames()
      expect(start - root.scrollTop).toBeGreaterThan(5_000)
      expect(writes.length).toBeLessThanOrEqual(2)
      expectWindow()
    } finally {
      Reflect.deleteProperty(root, 'scrollTop')
    }
  })

  it('bridges Tab and Shift-Tab gaps and still steps through days with the Page keys', async () => {
    await open()
    const [first, second] = heavyRows.map(keyOf)
    entry(first!)!.focus({ preventScroll: true })
    await scroll(34_000)
    expect(entry(second!)).toBeNull()

    await userEvent.keyboard('{Tab}')
    await expect.poll(() => document.activeElement?.getAttribute('data-entry')).toBe(second)
    expectWindow()

    await scroll(34_000)
    expect(entry(first!)).toBeNull()
    await userEvent.keyboard('{Shift>}{Tab}{/Shift}')
    await expect.poll(() => document.activeElement?.getAttribute('data-entry')).toBe(first)

    await userEvent.keyboard('{PageDown}')
    await expect.poll(() => document.activeElement?.getAttribute('data-day')).toBe(DAYS[2])
    await userEvent.keyboard('{PageUp}')
    await expect.poll(() => document.activeElement?.getAttribute('data-day')).toBe(HEAVY)
    expectWindow()
  })

  it('holds the same reading offset through a collection and a resize, then hands it to a surviving row', async () => {
    const { backend } = await open()
    await scroll(34_000)
    expectWindow()
    let read = reading()
    let current = heavyRows.findIndex(row => keyOf(row) === read.key)
    if (heavyRows[current]?.message.includes('TICKET-7')) {
      await scroll(scroller().scrollTop + read.top - line() + 1)
      read = reading()
      current = heavyRows.findIndex(row => keyOf(row) === read.key)
    }
    expect(heavyRows[current]?.message).not.toContain('TICKET-7')
    const successor = heavyRows.slice(current + 1).find(row => row.message.includes('TICKET-7'))!
    const titleHeight = (): number | undefined =>
      entry(keyOf(successor))?.querySelector('.entry-title')?.getBoundingClientRect().height
    const wideTitle = titleHeight()!

    await collect(backend, 40)
    await expect.poll(() => section().querySelector('.day-count')?.textContent).toBe('1040 events')
    await expectPlace(read.key, read.top)

    await page.viewport(402, 800)
    await frames(6)
    await expectPlace(read.key, read.top)
    expect(titleHeight()).toBeGreaterThan(wideTitle)

    TestBed.inject(ActivityViewStateService).setSearchQuery('TICKET-7')
    await expect.poll(() => section().querySelector('.day-count')?.textContent).toBe('348 events')
    expect(entry(read.key)).toBeNull()
    await expectPlace(keyOf(successor), read.top)
  })

  it('restores the exact offset on Back and Forward after the heavy day changes while away', async () => {
    const { backend, router } = await open()
    const history = TestBed.inject(Location)
    const timeline = document.querySelector('app-timeline')!

    await scroll(25_000)
    expectWindow()
    const back = reading()
    await router.navigateByUrl(`/week/${HEAVY}?${RANGE}`)
    await expect.poll(() => timeline.isConnected).toBe(false)
    await collect(backend, 20)
    history.back()
    await expect.poll(() => timeline.isConnected).toBe(true)
    await expectPlace(back.key, back.top)

    await router.navigateByUrl(`/week/${HEAVY}?${RANGE}`)
    await router.navigateByUrl(`/timeline?${RANGE}`)
    await expect.poll(() => scroller().scrollTop).toBe(0)
    await scroll(41_000)
    const forward = reading()
    history.back()
    await expect.poll(() => timeline.isConnected).toBe(false)
    await collect(backend, 40)
    history.forward()
    await expect.poll(() => timeline.isConnected).toBe(true)
    await expectPlace(forward.key, forward.top)
  })
})

describe('the Timeline whole-day cutoff', () => {
  it('draws all 150 rows, windows the day at 151 and expands it again under a filter', async () => {
    const { backend } = await open(150)
    expect(drawn()).toBe(150)
    await scroll(4_000)
    const read = reading()

    await collect(backend, 1, 150)
    await expect.poll(() => section().querySelector('.day-count')?.textContent).toBe('151 events')
    await expectPlace(read.key, read.top)
    expect(drawn()).toBeLessThan(150)

    TestBed.inject(ActivityViewStateService).setSearchQuery('TICKET-7')
    await expect.poll(drawn).toBe(51)
    expect(section().querySelector('.day-count')?.textContent).toBe('51 events')
  })
})
