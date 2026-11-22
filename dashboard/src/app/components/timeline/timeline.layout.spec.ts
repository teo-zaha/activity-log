/// <reference types="@vitest/browser-playwright" />
// The timeline in a real browser (`ng test --configuration layout`, Chromium): where the list is
// scrolled to, which day is active and what a finger pulling it does, which only a layout engine
// and its input pipeline can tell.
import { LiveAnnouncer } from '@angular/cdk/a11y'
import { TestBed } from '@angular/core/testing'
import type { ActivityLog, GitEntry } from '../../generated/activity'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cdp, page, userEvent } from 'vitest/browser'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
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

async function openTimeline(
  width = 1280,
  backend = new MockBackendTransport(),
  query = ''
): Promise<OpenApp> {
  await page.viewport(width, 800)
  backend.activityResponses = [log]
  backend.activityResponse = log
  backend.collectResponse = log
  const app = await openApp(`/timeline?${RANGE}${query}`, { backend })
  // The virtual list draws the days near the screen, and says how many there are.
  await expect
    .poll(() => document.querySelector('app-timeline .day-group')?.getAttribute('aria-setsize'))
    .toBe(String(DAYS))
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

/** A touch screen, as the browser's own input pipeline sees one. */
async function touchScreen(enabled: boolean): Promise<void> {
  await cdp().send('Emulation.setTouchEmulationEnabled', { enabled, maxTouchPoints: 5 })
}

interface Point {
  readonly x: number
  readonly y: number
}

/** What the pull indicator shows: `idle`, `pressed`, `pulling`, `armed`. */
function pullShown(): string | null {
  return document.querySelector('app-timeline .pull')?.getAttribute('data-pull') ?? null
}

/** Whether the pull indicator is out of sight, above the strip it comes out of. */
function indicatorHidden(): boolean {
  const strip = document.querySelector('app-timeline .pull')!.getBoundingClientRect()
  const indicator = document.querySelector('app-timeline .pull-indicator')!.getBoundingClientRect()
  return indicator.bottom <= strip.top
}

/** Where things are that must not move while a finger pulls: the page, the shell, the list. */
function positions(): (number | null)[] {
  return [
    document.scrollingElement?.scrollTop ?? 0,
    document.querySelector('app-shell-layout')!.getBoundingClientRect().top,
    scroller().scrollTop,
    document.querySelector('app-timeline .day-group')?.getBoundingClientRect().top ?? null,
  ]
}

/** Until the list has come to rest: it rises a little as it comes in. */
async function stillList(): Promise<void> {
  let before = ''
  await expect
    .poll(async () => {
      await frames()
      const now = JSON.stringify(positions())
      const still = now === before
      before = now
      return still
    })
    .toBe(true)
}

/**
 * Drags from `from` down by `down` pixels in steps, through the browser's input pipeline: as a
 * finger, or as a mouse with its button held. Returns where things were at each step, and what
 * the indicator showed just before the finger lifted.
 */
async function drag(
  input: 'touch' | 'mouse',
  from: Point,
  down: number
): Promise<{ seen: (number | null)[][]; beforeRelease: string | null }> {
  const at = (y: number): Point => ({ x: from.x, y: from.y + y })
  const send = async (type: 'start' | 'move' | 'end', point: Point): Promise<void> => {
    if (input === 'touch') {
      await cdp().send('Input.dispatchTouchEvent', {
        type: type === 'start' ? 'touchStart' : type === 'move' ? 'touchMove' : 'touchEnd',
        touchPoints: type === 'end' ? [] : [{ ...point, id: 1 }],
      })
    } else {
      await cdp().send('Input.dispatchMouseEvent', {
        type: type === 'start' ? 'mousePressed' : type === 'move' ? 'mouseMoved' : 'mouseReleased',
        ...point,
        button: 'left',
        buttons: type === 'end' ? 0 : 1,
        clickCount: 1,
      })
    }
  }
  const seen: (number | null)[][] = []
  await send('start', at(0))
  const steps = 12
  for (let step = 1; step <= steps; step++) {
    await send('move', at((down * step) / steps))
    await frames()
    seen.push(positions())
  }
  const beforeRelease = pullShown()
  await send('end', at(down))
  await frames()
  return { seen, beforeRelease }
}

/** The middle of the screen, whatever is over the list there. */
function onScreenCentre(): Point {
  return { x: innerWidth / 2, y: innerHeight / 2 }
}

/** A point on an entry of the list on screen, below the filter, clear of the screen's edges. */
function onEntry(): Point {
  const list = scroller().getBoundingClientRect()
  const point = { x: list.left + list.width / 2, y: list.top + 200 }
  expect(document.elementFromPoint(point.x, point.y)?.closest('.entry')).not.toBeNull()
  return point
}

describe('pulling the timeline to collect again', () => {
  afterEach(async () => {
    await touchScreen(false)
    await frames()
  })

  async function openPullable(
    options: { width?: number; backend?: MockBackendTransport; query?: string } = {}
  ): Promise<{ collect: ReturnType<typeof vi.spyOn>; app: OpenApp }> {
    await touchScreen(true)
    const app = await openTimeline(options.width ?? 402, options.backend, options.query)
    await app.settle()
    await expect.poll(() => TestBed.inject(ActivityDataService).isLoading()).toBe(false)
    await stillList()
    const data = TestBed.inject(ActivityDataService)
    return { collect: vi.spyOn(data, 'collect'), app }
  }

  it('collects once when a finger pulls past the threshold, and the page stays put', async () => {
    const backend = new MockBackendTransport()
    let answer = (): void => undefined
    const { collect } = await openPullable({ backend })
    backend.collectGate = new Promise(resolve => (answer = resolve))
    const announce = vi.spyOn(TestBed.inject(LiveAnnouncer), 'announce')
    const start = positions()

    const { seen, beforeRelease } = await drag('touch', onEntry(), 220)

    expect(beforeRelease).toBe('armed')
    // Said once, as the pull passed the threshold, not at every move past it.
    expect(announce.mock.calls.filter(([message]) => message === 'Release to collect')).toEqual([
      ['Release to collect', 'polite'],
    ])
    // Neither the page, the shell nor the list moved under the finger.
    for (const step of seen) expect(step).toEqual(start)
    expect(collect).toHaveBeenCalledOnce()
    // The finger lifted on an entry: its details stay closed.
    expect(TestBed.inject(ActivityViewStateService).detailsOpen()).toBe(false)
    answer()
  })

  it('goes back up once released past the threshold, leaving the collection to its status', async () => {
    const backend = new MockBackendTransport()
    let answer = (): void => undefined
    const { collect } = await openPullable({ backend })
    backend.collectGate = new Promise(resolve => (answer = resolve))

    await drag('touch', onEntry(), 220)

    expect(collect).toHaveBeenCalledOnce()
    expect(TestBed.inject(ActivityDataService).isLoading()).toBe(true)
    // The collection runs, and the indicator neither stays nor says so: the status pill does.
    expect(pullShown()).toBe('idle')
    await expect.poll(indicatorHidden).toBe(true)
    const indicator = document.querySelector('app-timeline .pull')!
    expect(indicator.textContent).not.toContain('Collecting')
    expect(indicator.querySelector('.u-spin')).toBeNull()
    // Another pull meanwhile starts none.
    expect((await drag('touch', onEntry(), 220)).beforeRelease).toBe('idle')
    answer()
    await expect.poll(() => TestBed.inject(ActivityDataService).isLoading()).toBe(false)
    expect(collect).toHaveBeenCalledOnce()
  })

  it('collects nothing for a short pull', async () => {
    const { collect } = await openPullable()

    const { beforeRelease } = await drag('touch', onEntry(), 40)

    expect(beforeRelease).toBe('pulling')
    await expect.poll(pullShown).toBe('idle')
    expect(collect).not.toHaveBeenCalled()
  })

  it('never pulls for a mouse dragging or a wheel turning at the top of the list', async () => {
    const { collect } = await openPullable()
    await touchScreen(false)
    const start = positions()

    const { seen } = await drag('mouse', onEntry(), 220)
    const { x, y } = onEntry()
    await cdp().send('Input.dispatchMouseEvent', {
      type: 'mouseWheel',
      x,
      y,
      deltaX: 0,
      deltaY: -400,
    })
    await frames()

    for (const step of seen) expect(step).toEqual(start)
    expect(pullShown()).toBe('idle')
    expect(collect).not.toHaveBeenCalled()
  })

  it('scrolls the list instead once it is scrolled down', async () => {
    const { collect } = await openPullable()
    await scrollTo(600)

    await drag('touch', onEntry(), 220)

    expect(scroller().scrollTop).toBeLessThan(600)
    expect(pullShown()).toBe('idle')
    expect(collect).not.toHaveBeenCalled()
  })

  it('leaves a finger on the day index to the index', async () => {
    // A touch screen wide enough for the index to show beside the list.
    const { collect } = await openPullable({ width: 1024 })
    const rail = document.querySelector('app-day-index')!.getBoundingClientRect()

    const { beforeRelease } = await drag(
      'touch',
      { x: rail.left + rail.width / 2, y: rail.top + 20 },
      220
    )

    expect(beforeRelease).toBe('idle')
    expect(collect).not.toHaveBeenCalled()
  })

  it('does not pull under the details', async () => {
    const event = `git/default/${entries[0]!.id}`
    const { collect, app } = await openPullable({ query: `&event=${encodeURIComponent(event)}` })
    expect(TestBed.inject(ActivityViewStateService).detailsOpen()).toBe(true)

    const { beforeRelease } = await drag('touch', onScreenCentre(), 220)

    expect(beforeRelease).toBe('idle')
    expect(collect).not.toHaveBeenCalled()
    await app.router.navigateByUrl(`/timeline?${RANGE}`)
    await stillList()
    expect((await drag('touch', onEntry(), 220)).beforeRelease).toBe('armed')
  })

  /** Pulls past the threshold and holds; the returned function lifts the finger. */
  async function holdPull(): Promise<() => Promise<void>> {
    const from = onEntry()
    await cdp().send('Input.dispatchTouchEvent', {
      type: 'touchStart',
      touchPoints: [{ ...from, id: 1 }],
    })
    for (const down of [20, 120, 220]) {
      await cdp().send('Input.dispatchTouchEvent', {
        type: 'touchMove',
        touchPoints: [{ x: from.x, y: from.y + down, id: 1 }],
      })
      await frames()
    }
    expect(pullShown()).toBe('armed')
    return async () => {
      await cdp().send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] })
      await frames()
    }
  }

  it('drops a pull when the details open over it meanwhile, as from a link', async () => {
    const { collect, app } = await openPullable()
    const event = `git/default/${entries[0]!.id}`
    const lift = await holdPull()

    await app.router.navigateByUrl(`/timeline?${RANGE}&event=${encodeURIComponent(event)}`)
    await frames()
    await lift()

    expect(pullShown()).toBe('idle')
    expect(collect).not.toHaveBeenCalled()
  })

  it('drops a pull when another view takes its place meanwhile, and pulls again after', async () => {
    const { collect, app } = await openPullable()
    const lift = await holdPull()

    await app.router.navigateByUrl(`/week/${NEWEST}?${RANGE}`)
    await frames()
    await lift()
    await app.router.navigateByUrl(`/timeline?${RANGE}`)
    await stillList()

    expect(pullShown()).toBe('idle')
    expect(collect).not.toHaveBeenCalled()
    expect((await drag('touch', onEntry(), 220)).beforeRelease).toBe('armed')
    await vi.waitFor(() => expect(collect).toHaveBeenCalledOnce())
  })

  it('does not pull while a collection runs', async () => {
    const backend = new MockBackendTransport()
    const { collect } = await openPullable({ backend })
    let answer = (): void => undefined
    backend.collectGate = new Promise(resolve => (answer = resolve))
    void TestBed.inject(ActivityDataService).collectShownRange()
    collect.mockClear()

    expect((await drag('touch', onEntry(), 220)).beforeRelease).toBe('idle')
    expect(collect).not.toHaveBeenCalled()
    answer()
  })

  it('does not pull without a source to collect', async () => {
    const backend = new MockBackendTransport()
    const { collect } = await openPullable({ backend })
    backend.providersResponse = {
      ...backend.providersResponse,
      providers: backend.providersResponse.providers.map(provider => ({
        ...provider,
        status: 'unavailable',
        usable: false,
        problem: 'unreachable',
        reason: 'The server does not answer',
        recheckable: true,
      })),
    }
    backend.publish()
    await expect.poll(() => TestBed.inject(ActivityDataService).anySourceReady()).toBe(false)
    await frames()

    expect((await drag('touch', onScreenCentre(), 220)).beforeRelease).toBe('idle')
    expect(collect).not.toHaveBeenCalled()
  })
})
