/// <reference types="@vitest/browser-playwright" />
// Where the reader's row stays as the Timeline's days change, in a real browser (`ng test
// --configuration layout`, Chromium): long days whose rows wrap, so a day's estimated height is
// not its drawn one, as with real activity.
import { Location } from '@angular/common'
import { TestBed } from '@angular/core/testing'
import type { ActivityLog, GitEntry } from '../../generated/activity'
import { afterEach, describe, expect, it } from 'vitest'
import { cdp, page } from 'vitest/browser'
import type { autoAnimate } from '@formkit/auto-animate'
import { provideAppRouter } from '../../app.routes'
import { parseEntryKey } from '../../providers/connection'
import { AUTO_ANIMATE } from '../auto-animate/auto-animate.directive'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { MockBackendTransport } from '../../testing/activity-fixtures'
import { openApp, type OpenApp } from '../../testing/app'
import { addDaysToKey, type DayKey } from '../../utils/local-day'

const NEWEST: DayKey = '2026-05-18'
const DAYS = 40
const ROWS = 30
const RANGE = `from=${addDaysToKey(NEWEST, -(DAYS - 1))}&to=${NEWEST}`
const days = Array.from({ length: DAYS }, (_, index) => addDaysToKey(NEWEST, -index))
/** Every third change wraps to two lines, and only those name a ticket. */
const LONG = 'TICKET-7 move the invoices to the new schema, with a title long enough to wrap'

function commit(day: DayKey, index: number, id = `${day.replaceAll('-', '')}${index}`): GitEntry {
  const [year, month, date] = day.split('-').map(Number)
  return {
    source: 'git',
    connection: 'default',
    id,
    timestamp: new Date(year!, month! - 1, date!, 23, 59 - index).toISOString(),
    repo: 'activity-log',
    hash: id,
    message: index % 3 === 0 ? `Change ${index}: ${LONG}` : `Change ${index}`,
    author: 'Dev',
  }
}

function logOf(entries: readonly GitEntry[]): ActivityLog {
  return {
    meta: {
      generated: `${NEWEST}T20:00:00Z`,
      start: new Date(2026, 3, 9).toISOString(),
      end: new Date(2026, 4, 19).toISOString(),
    },
    summary: {
      connections: [{ connection: { source: 'git', name: 'default' }, count: entries.length }],
      mergedCount: 0,
    },
    entries: [...entries],
  }
}

const entries = days.flatMap(day => Array.from({ length: ROWS }, (_, index) => commit(day, index)))

async function frames(count = 2): Promise<void> {
  for (let i = 0; i < count; i++) await new Promise(resolve => requestAnimationFrame(resolve))
}

/** After the frame now coming has been painted. */
async function painted(): Promise<void> {
  await new Promise(resolve => requestAnimationFrame(() => setTimeout(resolve)))
}

function scroller(): HTMLElement {
  const element = document.querySelector<HTMLElement>('app-timeline .timeline-scroll')
  if (!element) throw new Error('the timeline is on screen')
  return element
}

/** The line under the sticky day header, below the scroller's top. */
function line(): number {
  const host = document.querySelector('app-timeline')!
  return parseFloat(getComputedStyle(host).getPropertyValue('--day-header-top'))
}

/** Where a row is below the scroller's top; null where it is not drawn. */
function topOf(key: string): number | null {
  const row = [...scroller().querySelectorAll<HTMLElement>('.entry')].find(
    candidate => candidate.dataset['entry'] === key
  )
  return row ? row.getBoundingClientRect().top - scroller().getBoundingClientRect().top : null
}

/** The row read: the first one at or below the line. */
function reading(): { key: string; top: number; day: DayKey } {
  const top = scroller().getBoundingClientRect().top
  const row = [...scroller().querySelectorAll<HTMLElement>('.entry')].find(
    candidate => candidate.getBoundingClientRect().top - top >= line()
  )
  if (!row) throw new Error('a row is under the line')
  const day = days.find(shown => shown === row.closest('article')?.getAttribute('data-day'))
  return { key: row.dataset['entry']!, top: row.getBoundingClientRect().top - top, day: day! }
}

async function scrollTo(top: number): Promise<void> {
  const element = scroller()
  const before = element.scrollTop
  element.scrollTop = top
  if (element.scrollTop !== before) {
    await new Promise(resolve => element.addEventListener('scroll', resolve, { once: true }))
  }
  await frames()
}

/**
 * AutoAnimate as it is under reduced motion: off. It reads the preference once, as it loads, so
 * emulating reduced motion afterwards does not turn it off.
 */
const stillRows: typeof autoAnimate = parent => ({
  parent,
  enable: (): void => {},
  disable: (): void => {},
  isEnabled: (): boolean => false,
})

async function openTimeline(
  backend = new MockBackendTransport(),
  { motion = true } = {}
): Promise<OpenApp> {
  await page.viewport(1280, 800)
  const log = logOf(entries)
  backend.activityResponses = [log]
  backend.activityResponse = log
  backend.collectResponse = log
  const router = motion
    ? provideAppRouter()
    : [provideAppRouter(), { provide: AUTO_ANIMATE, useValue: stillRows }]
  const app = await openApp(`/timeline?${RANGE}`, { backend, router })
  await expect
    .poll(() => document.querySelector('app-timeline .day-group')?.getAttribute('aria-setsize'))
    .toBe(String(DAYS))
  return app
}

/** A collection that brings `count` more rows, the long kind, to each of `to`. */
async function collect(
  backend: MockBackendTransport,
  to: readonly DayKey[],
  count: number,
  { shown = true } = {}
): Promise<number> {
  const added = to.flatMap(day =>
    Array.from({ length: count }, (_, index) => commit(day, 3 * index, `${day}-new-${index}`))
  )
  backend.activityResponse = logOf([...added, ...entries])
  backend.publish()
  const total = entries.length + added.length
  if (shown) await shownCount(total)
  return total
}

/** Waits for the Timeline's summary to count `total` events. */
async function shownCount(total: number): Promise<void> {
  await expect
    .poll(() => document.querySelector('app-timeline .timeline-summary')?.textContent)
    .toContain(`${total} events`)
}

/** The row read once the list stands still: the days drawn by a jump are measured first. */
async function settledReading(): Promise<{ key: string; top: number; day: DayKey }> {
  let read = reading()
  for (let still = 0; still < 3;) {
    await painted()
    const next = reading()
    still = next.key === read.key && Math.abs(next.top - read.top) < 0.5 ? still + 1 : 0
    read = next
  }
  return read
}

/** The days before `day` in the list, nearest last. */
const above = (day: DayKey, count: number): DayKey[] =>
  days.slice(Math.max(0, days.indexOf(day) - count), days.indexOf(day))

async function reducedMotion(on: boolean): Promise<void> {
  await cdp().send('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-reduced-motion', value: on ? 'reduce' : '' }],
  })
}

describe('the reader’s place in the timeline', () => {
  afterEach(async () => {
    await reducedMotion(false)
    await frames()
  })

  it('keeps the row read in place on every frame as the drawn days just above it grow', async () => {
    await reducedMotion(true)
    const { backend } = await openTimeline(undefined, { motion: false })
    await scrollTo(9000)
    const read = await settledReading()

    const tops: (number | null)[] = []
    const watching = (async (): Promise<void> => {
      for (let i = 0; i < 12; i++) {
        await painted()
        tops.push(topOf(read.key))
      }
    })()
    await collect(backend, above(read.day, 2), 5)
    await watching

    // Every frame painted holds the row within a few pixels (under load one frame has been seen
    // 4 px off, put right with the next), and it ends where it was.
    const moved = tops.map(top => (top === null ? null : Math.round(top - read.top)))
    for (const offset of moved) expect(Math.abs(offset ?? Infinity)).toBeLessThanOrEqual(4)
    expect(moved.at(-1)).toBe(0)
  })

  it('keeps a scroll the user makes as a collection comes in', async () => {
    const { backend } = await openTimeline()
    await scrollTo(9000)
    const read = reading()
    const { x, y } = scroller().getBoundingClientRect()

    await cdp().send('Input.dispatchMouseEvent', {
      type: 'mouseWheel',
      x: x + 300,
      y: y + 300,
      deltaX: 0,
      deltaY: 120,
    })
    await collect(backend, [days[0]!, days[1]!], 6)
    await new Promise(resolve => setTimeout(resolve, 600))

    expect(topOf(read.key)! - read.top).toBeCloseTo(-120, -1)
  })

  it('goes on from the next row that matches when a filter takes the row read away', async () => {
    await openTimeline()
    // Deep in a long day, on a short row the filter leaves out.
    await scrollTo(9000)
    const order = entries.map(entry => `git/default/${entry.id}`)
    const matches = (key: string): boolean =>
      entries[order.indexOf(key)]?.message.includes('TICKET-7') ?? false
    let read = reading()
    while (matches(read.key)) {
      await scrollTo(scroller().scrollTop + 20)
      read = reading()
    }
    const next = entries
      .slice(order.indexOf(read.key) + 1)
      .find(entry => entry.message.includes('TICKET-7'))!

    TestBed.inject(ActivityViewStateService).setSearchQuery('TICKET-7')
    await expect
      .poll(() => document.querySelector('app-timeline .timeline-summary')?.textContent)
      .toContain('matching')
    await frames(4)

    expect(Math.abs((topOf(`git/default/${next.id}`) ?? Infinity) - read.top)).toBeLessThan(1.5)
  })

  it('shows the row read where it was as the details close after a collection', async () => {
    const { backend } = await openTimeline()
    await scrollTo(9000)
    const read = reading()
    const views = TestBed.inject(ActivityViewStateService)
    await views.openEvent(parseEntryKey(read.key)!)
    await expect.poll(() => views.detailsOpen()).toBe(true)

    await collect(backend, [days[0]!, days[1]!], 8)
    await frames()
    await views.closeDetail()
    await frames(4)

    expect(Math.abs((topOf(read.key) ?? Infinity) - read.top)).toBeLessThan(1.5)
  })

  it('shows the row read where it was on Back after a collection while away', async () => {
    const { backend, router } = await openTimeline()
    await scrollTo(9000)
    const read = reading()

    await router.navigateByUrl(`/week/${NEWEST}?${RANGE}`)
    await frames()
    const total = await collect(backend, [days[0]!, days[1]!], 8, { shown: false })
    await new Promise(resolve => setTimeout(resolve, 100))
    TestBed.inject(Location).back()
    await expect.poll(() => document.querySelector('app-timeline')?.isConnected).toBe(true)
    await shownCount(total)
    await frames(4)

    expect(Math.abs((topOf(read.key) ?? Infinity) - read.top)).toBeLessThan(1.5)
  })

  it('keeps the focus on an entry as the list scrolls far away from it', async () => {
    await openTimeline()
    const entry = scroller().querySelector<HTMLElement>('.entry')!
    entry.focus()

    await scrollTo(scroller().scrollHeight)
    await frames(4)

    expect(document.activeElement).toBe(entry)
    expect(entry.isConnected).toBe(true)
  })
})
