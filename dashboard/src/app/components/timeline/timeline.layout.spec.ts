/// <reference types="@vitest/browser-playwright" />
// The timeline in a real browser (`ng test --configuration layout`, Chromium): where the list is
// scrolled to and which day is active, which only a layout engine can tell.
import type { ActivityLog, GitEntry } from '../../generated/activity'
import { afterEach, describe, expect, it } from 'vitest'
import { page, userEvent } from 'vitest/browser'
import { MockBackendTransport } from '../../testing/activity-fixtures'
import { openApp, type OpenApp } from '../../testing/app'
import { addDaysToKey, longDayLabel, type DayKey } from '../../utils/local-day'

const NEWEST: DayKey = '2026-05-18'
const DAYS = 30
const OLDEST = addDaysToKey(NEWEST, -(DAYS - 1))
const RANGE = `from=${OLDEST}&to=${NEWEST}`
/** The last days hold one entry each: too short to scroll up to the line under the header. */
const SHORT_DAYS = 3

/** The days the list shows, newest first. */
const days = Array.from({ length: DAYS }, (_, index) => addDaysToKey(NEWEST, -index))

function commit(day: DayKey, index: number): GitEntry {
  const [year, month, date] = day.split('-').map(Number)
  const hash = `${day.replaceAll('-', '')}${index}`
  return {
    source: 'git',
    connection: 'default',
    id: hash,
    timestamp: new Date(year!, month! - 1, date!, 9 + index).toISOString(),
    repo: 'activity-log',
    hash,
    message: `Change ${index + 1} of ${day}`,
    author: 'Dev',
  }
}

const entries = days.flatMap((day, dayIndex) =>
  Array.from({ length: dayIndex >= DAYS - SHORT_DAYS ? 1 : 6 }, (_, index) => commit(day, index))
)

const log: ActivityLog = {
  meta: {
    generated: `${NEWEST}T20:00:00Z`,
    start: new Date(2026, 3, 19).toISOString(),
    end: new Date(2026, 4, 19).toISOString(),
  },
  summary: {
    connections: [{ connection: { source: 'git', name: 'default' }, count: entries.length }],
    mergedCount: 0,
  },
  entries,
}

/** Two frames: what a change lays out and what observes it has run. */
async function frames(): Promise<void> {
  for (let i = 0; i < 2; i++) await new Promise(resolve => requestAnimationFrame(resolve))
}

function scroller(): HTMLElement {
  const element = document.querySelector<HTMLElement>('app-timeline .timeline-scroll')
  if (!element) throw new Error('the timeline is on screen')
  return element
}

/** The day the day index shows as active, as it reads it out. */
function activeDay(): string | null {
  return document.querySelector('app-day-index')?.getAttribute('aria-valuetext') ?? null
}

const label = (day: DayKey, count: number): string =>
  `${longDayLabel(day)}, ${count} ${count === 1 ? 'event' : 'events'}`
const labelOf = (index: number): string => label(days[index]!, index >= DAYS - SHORT_DAYS ? 1 : 6)

/** Scrolls the list as a program does, and waits for what follows the scroll. */
async function scrollTo(top: number): Promise<void> {
  const element = scroller()
  const before = element.scrollTop
  element.scrollTop = top
  // The browser sends the scroll event with the next frame, if the list moved at all.
  if (element.scrollTop !== before) {
    await new Promise(resolve => element.addEventListener('scroll', resolve, { once: true }))
  }
  await frames()
}

async function openTimeline(): Promise<OpenApp> {
  await page.viewport(1280, 800)
  const backend = new MockBackendTransport()
  backend.activityResponses = [log]
  backend.activityResponse = log
  const app = await openApp(`/timeline?${RANGE}`, { backend })
  await expect.poll(() => document.querySelectorAll('app-timeline .day-group').length).toBe(DAYS)
  await expect.poll(activeDay).toBe(labelOf(0))
  return app
}

describe('the timeline in a browser', () => {
  afterEach(async () => {
    await frames()
  })

  it('keeps where the list was scrolled to over a trip to the week view', async () => {
    const { router } = await openTimeline()
    const timeline = document.querySelector('app-timeline')
    await scrollTo(1500)
    const scrolled = scroller().scrollTop
    const active = activeDay()
    expect(active).not.toBe(labelOf(0))

    await router.navigateByUrl(`/week/${NEWEST}?${RANGE}`)
    await frames()
    expect(timeline?.isConnected).toBe(false)
    await router.navigateByUrl(`/timeline?${RANGE}`)
    await frames()

    expect(document.querySelector('app-timeline')).toBe(timeline)
    expect(scrolled).toBeGreaterThan(1000)
    expect(scroller().scrollTop).toBe(scrolled)
    expect(activeDay()).toBe(active)
  })

  it('makes the day under the header active as the list scrolls', async () => {
    await openTimeline()
    const third = document.querySelector<HTMLElement>(`[data-day="${days[2]}"]`)!
    const headerTop = parseFloat(
      getComputedStyle(document.querySelector('app-timeline')!).getPropertyValue('--day-header-top')
    )

    await scrollTo(third.offsetTop - headerTop)
    await expect.poll(activeDay).toBe(labelOf(2))

    await scrollTo(0)
    await expect.poll(activeDay).toBe(labelOf(0))
  })

  it('keeps a jumped-to day active until the user scrolls', async () => {
    await openTimeline()
    const rail = document.querySelector<HTMLElement>('app-day-index')!
    rail.focus()

    // The second-oldest day cannot reach the line: at the end of the list the line has slid to
    // the bottom edge, onto the oldest day.
    await userEvent.keyboard('{End}')
    await expect.poll(activeDay).toBe(labelOf(DAYS - 1))
    await userEvent.keyboard('{ArrowUp}')
    for (let i = 0; i < 6; i++) await frames()
    expect(activeDay()).toBe(labelOf(DAYS - 2))
    const end = scroller().scrollHeight - scroller().clientHeight
    expect(scroller().scrollTop).toBeGreaterThanOrEqual(end - 1)

    scroller().dispatchEvent(new WheelEvent('wheel', { deltaY: -100 }))
    await scrollTo(0)
    await expect.poll(activeDay).toBe(labelOf(0))
  })

  it('makes each of the last days active in turn at the end of the list', async () => {
    await openTimeline()
    const element = scroller()
    const end = element.scrollHeight - element.clientHeight
    const seen: (string | null)[] = []

    for (let top = end - element.clientHeight; top <= end; top += 20) {
      await scrollTo(top)
      seen.push(activeDay())
    }
    await scrollTo(end)
    seen.push(activeDay())

    const order = seen.filter((day, index) => day !== seen[index - 1])
    expect(order.slice(-SHORT_DAYS)).toEqual(
      Array.from({ length: SHORT_DAYS }, (_, i) => labelOf(DAYS - SHORT_DAYS + i))
    )
  })
})
