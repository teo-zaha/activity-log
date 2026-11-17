import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { DayIndexItem } from '../../../models/day-index'
import { stubLayoutObservers } from '../../../testing/observers'
import { buildDayIndex } from '../../../utils/day-index'
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

describe('DayIndexComponent', () => {
  beforeEach(() => stubLayoutObservers())
  afterEach(() => vi.unstubAllGlobals())

  it('is a vertical slider describing the active day', async () => {
    const { rail } = await renderRail('2026-11-16')

    expect(rail.getAttribute('aria-orientation')).toBe('vertical')
    expect(rail.getAttribute('aria-valuetext')).toBe('Monday, 16 November 2026, 9 events')
    expect(rail.getAttribute('aria-valuenow')).toBe('3')
    expect(rail.getAttribute('aria-valuemax')).toBe('3')
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
