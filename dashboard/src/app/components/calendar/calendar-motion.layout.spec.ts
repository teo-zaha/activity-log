/// <reference types="@vitest/browser-playwright" />
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cdp, page } from 'vitest/browser'
import type { ActivityLog, CalendarEntry } from '../../generated/activity'
import { MockBackendTransport } from '../../testing/activity-fixtures'
import { openApp } from '../../testing/app'
import { APP_RENDER_TEST } from '../../testing/timeouts'

const DAY = '2026-05-20'
const RANGE = 'from=2026-05-12&to=2026-05-28'
const BLOCK = '.cal-event, .cal-overflow, .cal-allday-bar'
const VIEWS = ['day', 'workweek', 'week'] as const

function meeting(id: string, day: number, hour: number, durationMinutes = 60): CalendarEntry {
  return {
    source: 'calendar',
    connection: 'default',
    id,
    title: id,
    timestamp: new Date(2026, 4, day, hour).toISOString(),
    durationMinutes,
    meetingWeight: 'busy',
  }
}

const kept = meeting('Kept meeting', 20, 10)
const incoming = meeting('Earlier meeting', 20, 9, 90)
const previousDay = meeting('Previous day meeting', 19, 10)
const previousWeek = meeting('Previous week meeting', 13, 10)
const overnight = meeting('Overnight meeting', 20, 23, 120)

function log(entries: CalendarEntry[]): ActivityLog {
  return {
    meta: {
      generated: '2026-05-20T20:00:00Z',
      start: new Date(2026, 4, 12).toISOString(),
      end: new Date(2026, 4, 29).toISOString(),
    },
    summary: {
      connections: [{ connection: { source: 'calendar', name: 'default' }, count: entries.length }],
      mergedCount: 0,
    },
    entries,
  }
}

function block(entry: CalendarEntry): HTMLElement | null {
  const rendered = document.querySelectorAll<HTMLElement>(
    'app-calendar-day-column .cal-event, app-calendar-all-day-row .cal-allday-bar'
  )
  return (
    [...rendered].find(
      button => button.querySelector('.cal-event-title')?.textContent === entry.id
    ) ?? null
  )
}

function frames(): Promise<void> {
  return new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(() => resolve())))
}

async function open(view: (typeof VIEWS)[number], entries = [kept]): Promise<MockBackendTransport> {
  await page.viewport(1280, 800)
  const backend = new MockBackendTransport()
  backend.providersResponse = {
    ...backend.providersResponse,
    providers: [
      {
        connection: { source: 'calendar', name: 'default' },
        label: 'Demo calendar',
        status: 'available',
        usable: true,
        recheckable: false,
        supportedOs: ['macos'],
        checkedAt: '2026-05-20T08:00:00Z',
      },
    ],
  }
  backend.activityResponse = log(entries)
  backend.activityResponses = [backend.activityResponse]
  await openApp(`/${view}/${DAY}?${RANGE}`, { backend })
  await expect.poll(() => block(entries[0]!)).not.toBeNull()
  await frames()
  return backend
}

function publish(backend: MockBackendTransport, ...entries: CalendarEntry[]): void {
  backend.activityResponse = log(entries)
  backend.publish()
}

/** Hold real animations partway through, so a slow frame cannot miss them. */
function holdBlockAnimations(): Map<HTMLElement, Animation> {
  const seen = new Map<HTMLElement, Animation>()
  const animate = Element.prototype.animate
  vi.spyOn(Element.prototype, 'animate').mockImplementation(function (
    this: Element,
    keyframes: Keyframe[] | PropertyIndexedKeyframes | null,
    options?: number | KeyframeAnimationOptions
  ) {
    const animation = animate.call(this, keyframes, options)
    if (this instanceof HTMLElement && this.matches(BLOCK)) {
      animation.pause()
      animation.currentTime = Number(animation.effect?.getTiming().duration) * 0.75
      seen.set(this, animation)
    }
    return animation
  })
  return seen
}

function expectPartialOpacity(element: HTMLElement): void {
  const opacity = Number(getComputedStyle(element).opacity)
  expect(opacity).toBeGreaterThan(0)
  expect(opacity).toBeLessThan(1)
}

async function reduceMotion(reduce: boolean): Promise<void> {
  await cdp().send('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-reduced-motion', value: reduce ? 'reduce' : 'no-preference' }],
  })
  await frames()
}

afterEach(async () => {
  vi.restoreAllMocks()
  await reduceMotion(false)
})

describe('calendar collection motion', APP_RENDER_TEST, () => {
  it.each(VIEWS)(
    'slides the existing %s block without losing focus and fades in the new overlap',
    async view => {
      const seen = holdBlockAnimations()
      const backend = await open(view)
      const original = block(kept)!
      const before = original.getBoundingClientRect()
      original.focus({ preventScroll: true })
      expect(seen.size).toBe(0)

      publish(backend, incoming, kept)
      await expect.poll(() => seen.size).toBe(2)
      const added = block(incoming)!
      expect(block(kept)).toBe(original)
      expect(document.activeElement).toBe(original)
      expect(getComputedStyle(original).transform).not.toBe('none')
      expectPartialOpacity(added)

      for (const animation of seen.values()) animation.finish()
      await expect.poll(() => getComputedStyle(original).transform).toBe('none')
      expect(original.getBoundingClientRect().left).toBeGreaterThan(before.left)
      expect(original.getBoundingClientRect().width).toBeLessThan(before.width)
      expect(document.activeElement).toBe(original)
      expect(Number(getComputedStyle(added).opacity)).toBe(1)
    }
  )

  it('moves a focused meeting when a collection only changes the geometry of existing blocks', async () => {
    const seen = holdBlockAnimations()
    const backend = await open('day', [kept, incoming])
    const original = block(kept)!
    original.focus({ preventScroll: true })

    publish(backend, { ...incoming, timestamp: new Date(2026, 4, 20, 12).toISOString() }, kept)
    await expect.poll(() => seen.get(original)).toBeDefined()
    expect(getComputedStyle(original).transform).not.toBe('none')
    expect(document.activeElement).toBe(original)
  })

  it('fades removed timed blocks and the last all-day bar out as inert visual copies', async () => {
    const seen = holdBlockAnimations()
    const backend = await open('day', [kept, incoming])
    publish(backend, kept, incoming, overnight)
    await expect.poll(() => seen.has(block(overnight)!)).toBe(true)
    expectPartialOpacity(block(overnight)!)
    for (const animation of seen.values()) animation.finish()
    await frames()
    seen.clear()

    publish(backend, kept)
    const copies = (): HTMLElement[] => [
      ...document.querySelectorAll<HTMLElement>(
        '.cal-block-ghosts[inert][aria-hidden="true"] button'
      ),
    ]
    await expect.poll(copies).toHaveLength(2)
    expect(
      copies()
        .map(node => node.querySelector('.cal-event-title')?.textContent?.trim())
        .sort()
    ).toEqual([incoming.id, overnight.id])
    for (const copy of copies()) {
      expectPartialOpacity(copy)
      seen.get(copy)!.finish()
    }
    await expect.poll(copies).toHaveLength(0)
  })

  it.each(VIEWS)('keeps %s blocks still through previous and next navigation', async view => {
    const seen = holdBlockAnimations()
    await open(view, [kept, previousDay, previousWeek])
    const unit = view === 'day' ? 'day' : 'week'
    await page.getByRole('button', { name: `Previous ${unit}` }).click()
    await expect.poll(() => block(view === 'day' ? previousDay : previousWeek)).not.toBeNull()
    await page.getByRole('button', { name: `Next ${unit}` }).click()
    await expect.poll(() => block(kept)).not.toBeNull()
    await frames()
    expect(seen.size).toBe(0)
  })

  it('keeps blocks still through zoom, resize and source toggles', async () => {
    const seen = holdBlockAnimations()
    await open('day')
    const original = block(kept)!
    const originalTop = original.offsetTop
    await page.getByRole('button', { name: 'Zoom in' }).click()
    await expect.poll(() => original.offsetTop).toBeGreaterThan(originalTop)
    const originalWidth = original.offsetWidth
    await page.viewport(980, 750)
    await expect.poll(() => original.offsetWidth).toBeLessThan(originalWidth)

    const source = page.getByRole('button', { name: /Demo calendar/ })
    await source.click()
    await expect.poll(() => block(kept)).toBeNull()
    await source.click()
    await expect.poll(() => block(kept)).not.toBeNull()
    await frames()
    expect(seen.size).toBe(0)
  })

  it('draws a collection update immediately with reduced motion', async () => {
    const seen = holdBlockAnimations()
    const backend = await open('day')
    await reduceMotion(true)
    publish(backend, incoming, kept)
    await expect.poll(() => block(incoming)).not.toBeNull()
    await frames()
    expect(seen.size).toBe(0)
  })

  it('skips disjoint replacements and applies one animation budget across the week', async () => {
    const seen = holdBlockAnimations()
    const backend = await open('week')
    publish(backend, incoming)
    await expect.poll(() => block(incoming)).not.toBeNull()
    await frames()
    expect(seen.size).toBe(0)

    const bulk = Array.from({ length: 35 }, (_, index) =>
      meeting(`Bulk ${index}`, 18 + Math.floor(index / 5), 8 + (index % 5), 30)
    )
    publish(backend, incoming, ...bulk)
    await expect.poll(() => block(bulk.at(-1)!)).not.toBeNull()
    await frames()
    expect(seen.size).toBe(0)
  })
})
