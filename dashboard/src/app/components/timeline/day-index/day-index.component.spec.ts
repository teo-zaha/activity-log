import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DayIndexItem } from '../../../models/day-index'
import { stubLayoutObservers } from '../../../testing/observers'
import { buildDayIndex } from '../../../utils/day-index'
import { type DayKey, localDateKey } from '../../../utils/local-day'
import { DayIndexComponent } from './day-index.component'

// Mon 16 Nov to Mon 2 Nov 2026; the weekend of 14–15 Nov has no events.
const items: DayIndexItem[] = buildDayIndex(
  '2026-11-02',
  '2026-11-16',
  new Map([
    ['2026-11-16', 9],
    ['2026-11-13', 2],
    ['2026-11-09', 1],
    ['2026-11-02', 4],
  ]),
  '2026-11-17'
)

async function renderRail(activeDay: string): Promise<{ rail: HTMLElement; jumps: string[] }> {
  const jumps: string[] = []
  await render(DayIndexComponent, {
    inputs: { items, activeDay },
    on: { jump: (day: string) => jumps.push(day) },
  })
  const rail = screen.getByRole('slider', { name: 'Jump to day' })
  rail.focus()
  return { rail, jumps }
}

// 18 Nov 2023 to 17 Nov 2026, with events on the 1st and 30th of every month and today.
const threeYears: DayIndexItem[] = ((): DayIndexItem[] => {
  const counts = new Map<DayKey, number>([['2026-11-17', 2]])
  for (let year = 2023; year <= 2026; year++) {
    for (let month = 1; month <= 12; month++) {
      counts.set(localDateKey(new Date(year, month - 1, 1)), 1)
      if (month !== 2) counts.set(localDateKey(new Date(year, month - 1, 30)), 1)
    }
  }
  return buildDayIndex('2023-11-18', '2026-11-17', counts, '2026-11-17')
})()

/** Renders `items` on a track `height` px high, with the layout jsdom lacks stubbed. */
async function renderLongRail(
  items: DayIndexItem[],
  activeDay: string,
  height: number
): Promise<{ rail: HTMLElement; jumps: string[] }> {
  vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(height)
  vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(
    new DOMRect(0, 0, 30, height + 12)
  )
  // jsdom has no pointer capture.
  HTMLElement.prototype.setPointerCapture = vi.fn()
  const jumps: string[] = []
  await render(DayIndexComponent, {
    inputs: { items, activeDay },
    on: { jump: (day: string) => jumps.push(day) },
  })
  const rail = screen.getByRole('slider', { name: 'Jump to day' })
  rail.focus()
  return { rail, jumps }
}

describe('DayIndexComponent', () => {
  beforeEach(() => stubLayoutObservers())
  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('is a vertical slider describing the active day', async () => {
    const { rail } = await renderRail('2026-11-16')

    expect(rail.getAttribute('aria-orientation')).toBe('vertical')
    expect(rail.getAttribute('aria-valuetext')).toBe('Monday, 16 November 2026, 9 events')
    expect(rail.getAttribute('aria-valuenow')).toBe('3')
    expect(rail.getAttribute('aria-valuemax')).toBe('3')
  })

  it('keeps the pill on the newest day clear of the top month label when dense', async () => {
    // 15 days in 80 px: about 4 px a day, a dense rail.
    const { rail } = await renderLongRail(items, '2026-11-16', 80)
    expect(rail.getAttribute('data-density')).toBe('dense')

    const pill = rail.querySelector<HTMLElement>('.pill')!
    const pillTop = parseFloat(pill.style.getPropertyValue('--y')) - 11
    // The 6 px track inset plus the 12 px month label.
    expect(pillTop).toBeGreaterThanOrEqual(18)
  })

  it('hides the active row under the pill and holds the hovered day out in a bump', async () => {
    vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(600)
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 30, 600))
    const { rail } = await renderRail('2026-11-13')
    expect(rail.getAttribute('data-density')).toBe('full')
    expect(rail.querySelector('.bulge')).toBeNull()

    rail.dispatchEvent(
      Object.assign(new MouseEvent('pointermove', { clientY: 200 }), { pointerType: 'mouse' })
    )
    // The day a press would land on is marked to grow, alone, on the next frame.
    await vi.waitFor(() => expect(rail.querySelectorAll('.day.is-preview')).toHaveLength(1))
    const preview = rail.querySelector('.day.is-preview')!
    expect(preview.classList.contains('is-empty')).toBe(false)
    expect(rail.querySelector('.bulge')).not.toBeNull()

    const active = rail.querySelectorAll('.day.is-active')
    expect(active).toHaveLength(1)
    expect(active[0].textContent?.trim()).toBe('13')
  })

  it('grows the pill out of the rail on the active day, and keeps the bump off it beside it', async () => {
    // Events on every day, so the pointer lands on the day under it.
    const busy = buildDayIndex(
      '2026-11-02',
      '2026-11-16',
      new Map(items.flatMap(item => (item.kind === 'day' ? [[item.key, 1] as const] : []))),
      '2026-11-17'
    )
    const { rail } = await renderLongRail(busy, '2026-11-13', 600)
    const pill = rail.querySelector<HTMLElement>('.pill')!
    const pillY = parseFloat(pill.style.getPropertyValue('--y'))
    const hover = (clientY: number): void => {
      rail.dispatchEvent(
        Object.assign(new MouseEvent('pointermove', { clientY }), { pointerType: 'mouse' })
      )
    }

    // On the active day the pill itself sticks out, and no bump shows behind it.
    hover(pillY)
    await vi.waitFor(() => expect(pill.classList.contains('is-held')).toBe(true))
    expect(rail.querySelector('.bulge')).toBeNull()

    // On the day above, the bump shows: the pill stays, and the bump stops short of it.
    hover(pillY - 18)
    await vi.waitFor(() => expect(rail.querySelector('.bulge')).not.toBeNull())
    expect(pill.classList.contains('is-held')).toBe(false)
    expect(rail.querySelector('.day.is-preview')?.textContent?.trim()).toBe('14')
    expect(parseFloat(pill.style.getPropertyValue('--y'))).toBe(pillY)
    const bulge = rail.querySelector<HTMLElement>('.bulge')!
    // Shorter beside the pill, evenly, so the day stays in its middle.
    const half = parseFloat(bulge.style.getPropertyValue('--half'))
    expect(half).toBeLessThan(15)
    expect(parseFloat(bulge.style.getPropertyValue('--y')) + half).toBeLessThanOrEqual(pillY - 11)
  })

  it('keeps the full pill on the newest day while nothing is hovered', async () => {
    // The newest day is the first row of the rail.
    const { rail } = await renderLongRail(items, '2026-11-16', 260)
    const pill = rail.querySelector<HTMLElement>('.pill')!
    expect(parseFloat(pill.style.getPropertyValue('--pill-h'))).toBe(22)
  })

  it('makes the pill thinner beside a magnified day and keeps the day on its own row', async () => {
    const busy = buildDayIndex(
      '2026-11-02',
      '2026-11-16',
      new Map(items.flatMap(item => (item.kind === 'day' ? [[item.key, 1] as const] : []))),
      '2026-11-17'
    )
    // About 15 px per day: no room for a bump between the pill and the day's row.
    const { rail } = await renderLongRail(busy, '2026-11-13', 260)
    expect(['full', 'sparse']).toContain(rail.getAttribute('data-density'))
    const pill = rail.querySelector<HTMLElement>('.pill')!
    const pillY = parseFloat(pill.style.getPropertyValue('--y'))
    const hover = (clientY: number): void => {
      rail.dispatchEvent(
        Object.assign(new MouseEvent('pointermove', { clientY }), { pointerType: 'mouse' })
      )
    }
    hover(pillY)
    await vi.waitFor(() => expect(pill.classList.contains('is-held')).toBe(true))

    // The day above the pill.
    hover(pillY - 14)
    await vi.waitFor(() => expect(rail.querySelector('.bulge')).not.toBeNull())
    const bulge = rail.querySelector<HTMLElement>('.bulge')!
    const half = parseFloat(bulge.style.getPropertyValue('--half'))
    const y = parseFloat(bulge.style.getPropertyValue('--y'))
    // The pill gets thinner to make room; the bump stays big enough for the magnified number
    // and centred on the day's own row, so the number sits in its middle.
    const pillHeight = parseFloat(pill.style.getPropertyValue('--pill-h'))
    expect(pillHeight).toBeLessThan(22)
    expect(half).toBeGreaterThanOrEqual(12)
    const preview = rail.querySelector<HTMLElement>('.day.is-preview')!
    expect(preview.textContent?.trim()).toBe('14')
    expect(preview.getAttribute('style') ?? '').not.toContain('--shift')
    // Beside the pill, on the side away from it.
    expect(y).toBeLessThan(pillY)
  })

  it('shows months instead of days on a long range and still lands on a day', async () => {
    // 1 Aug 2025 to 17 Nov 2026 in 300 px: well under 3 px per day.
    const long = buildDayIndex(
      '2025-08-01',
      '2026-11-17',
      new Map([
        ['2026-11-17', 3],
        ['2026-03-10', 5],
        ['2025-12-24', 1],
      ]),
      '2026-11-17'
    )
    const { rail, jumps } = await renderLongRail(long, '2026-11-17', 300)

    // 16 rows of 18.75 px: names and bars, the year on January, and room for the pill.
    expect(rail.getAttribute('data-density')).toBe('months-compact')
    const rows = [...rail.querySelectorAll('.month-row')]
    expect(rows).toHaveLength(16)
    expect(rows[10].textContent?.trim()).toBe('’26')
    expect(rail.querySelector('.pill')?.textContent?.trim()).toBe('Nov')

    // March 2026 is the 9th row of 16 (rows of 18.75 px from y = 6).
    const pressAt = 6 + 8.5 * 18.75
    rail.dispatchEvent(new MouseEvent('pointerdown', { clientY: pressAt, button: 0 }))

    expect(jumps).toEqual(['2026-03-10'])
  })

  it('labels quarters and years on a thin rail and marks the active month beside its row', async () => {
    // Three years in 300 px: about 8 px per month.
    const { rail } = await renderLongRail(threeYears, '2026-11-17', 300)

    expect(rail.getAttribute('data-density')).toBe('quarters')
    const labels = [...rail.querySelectorAll('.month-name')].map(label => label.textContent?.trim())
    expect(labels.slice(0, 4)).toEqual(['Oct', 'Jul', 'Apr', '2026'])
    expect(rail.querySelectorAll('.month-row.is-tick').length).toBeGreaterThan(20)
    // Too thin for the pill: a bare marker, and no label in it.
    const pill = rail.querySelector('.pill')
    expect(pill?.classList.contains('is-bare')).toBe(true)
    expect(pill?.textContent?.trim()).toBe('')
    expect(rail.classList.contains('has-bare-pill')).toBe(true)
  })

  it('keeps a short span together at the top instead of stretching it', async () => {
    // 15 days in 600 px would be 38 px each; they stay at 24 px.
    const { rail } = await renderLongRail(items, '2026-11-02', 600)

    expect(rail.getAttribute('data-density')).toBe('full')
    const pill = rail.querySelector<HTMLElement>('.pill')!
    // The 6 px inset, the 12 px month label and 14 rows of 24 px above the oldest day's centre.
    expect(parseFloat(pill.style.getPropertyValue('--y'))).toBe(6 + 12 + 14 * 24 + 12)
  })

  it('pages past a gap of more than two weeks to the next day with events', async () => {
    const sparse = buildDayIndex(
      '2026-09-01',
      '2026-11-16',
      new Map([
        ['2026-11-16', 1],
        ['2026-10-01', 1],
        ['2026-09-01', 1],
      ]),
      '2026-11-17'
    )
    // 77 days in 600 px: about 7 px a day, a rail of days.
    const { rail, jumps } = await renderLongRail(sparse, '2026-11-16', 600)
    expect(rail.getAttribute('data-density')).toBe('sparse')

    await userEvent.keyboard('{PageDown}')
    expect(jumps).toEqual(['2026-10-01'])
  })

  it('jumps to the newest day only with the up and down arrows when no day is active', async () => {
    const { jumps } = await renderRail('2026-11-20')

    await userEvent.keyboard('{ArrowLeft}{ArrowRight}')
    expect(jumps).toEqual([])
    await userEvent.keyboard('{ArrowDown}')
    expect(jumps).toEqual(['2026-11-16'])
  })

  it('draws no rows until the track is measured', async () => {
    // jsdom has no layout: the track is 0 px high.
    await render(DayIndexComponent, { inputs: { items, activeDay: '2026-11-16' } })
    const rail = screen.getByRole('slider', { name: 'Jump to day' })

    expect(rail.querySelectorAll('.day, .month-row')).toHaveLength(0)
    expect(rail.querySelector('.pill')).toBeNull()
  })

  it('pages by month on a rail of months', async () => {
    const { jumps } = await renderLongRail(threeYears, '2026-11-17', 300)

    await userEvent.keyboard('{PageDown}')
    expect(jumps).toEqual(['2026-10-30'])
  })

  it('keeps the bump on its day while the pointer moves within that day', async () => {
    vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(600)
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockReturnValue(new DOMRect(0, 0, 30, 600))
    const { rail } = await renderRail('2026-11-13')
    const bulgeAt = async (clientY: number): Promise<string | undefined> => {
      rail.dispatchEvent(
        Object.assign(new MouseEvent('pointermove', { clientY }), { pointerType: 'mouse' })
      )
      const bulge = (): string | undefined =>
        rail.querySelector<HTMLElement>('.bulge')?.style.getPropertyValue('--y')
      // Applied on the next frame; the second move finds the first one's bump already there.
      await new Promise(resolve => setTimeout(resolve, 50))
      await vi.waitFor(() => expect(bulge()).toBeTruthy())
      return bulge()
    }

    const first = await bulgeAt(200)
    const second = await bulgeAt(204)

    expect(second).toBe(first)
    // On the day, not under the pointer.
    expect(first).not.toBe('200px')
  })

  it('steps over empty days with the arrow keys', async () => {
    const { jumps } = await renderRail('2026-11-16')

    await userEvent.keyboard('{ArrowDown}')

    expect(jumps).toEqual(['2026-11-13'])
  })

  it('steps to the newer day with events on ArrowUp', async () => {
    const { jumps } = await renderRail('2026-11-09')

    await userEvent.keyboard('{ArrowUp}')

    expect(jumps).toEqual(['2026-11-13'])
  })

  it('jumps a week with PageDown and to either end with Home and End', async () => {
    const { jumps } = await renderRail('2026-11-13')

    await userEvent.keyboard('{PageDown}{Home}{End}')

    // A week before Fri 13 is Fri 6, which has no events: the nearest day with events is Mon 9.
    expect(jumps).toEqual(['2026-11-09', '2026-11-16', '2026-11-02'])
  })
})
