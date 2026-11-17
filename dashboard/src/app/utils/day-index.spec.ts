import { describe, expect, it } from 'vitest'
import type { DayIndexDay, DayIndexItem } from '../models/day-index'
import { buildDayIndex, densityFor, nearestDayWithEntries, slotAt } from './day-index'

function daysOf(items: readonly DayIndexItem[]): DayIndexDay[] {
  return items.filter((item): item is DayIndexDay => item.kind === 'day')
}

describe('buildDayIndex', () => {
  it('lists every day newest first with a heading per month and the year where it changes', () => {
    const items = buildDayIndex('2025-12-30', '2026-01-02', new Map(), '2026-01-02')

    expect(items.map(item => (item.kind === 'month' ? item.label : item.key))).toEqual([
      'Jan',
      '2026-01-02',
      '2026-01-01',
      '’25',
      '2025-12-31',
      '2025-12-30',
    ])
  })

  it('cuts every month label to three letters', () => {
    const [heading] = buildDayIndex('2026-09-01', '2026-09-01', new Map(), '2026-09-01')

    expect(heading).toEqual({ kind: 'month', key: '2026-09', label: 'Sep' })
  })

  it('marks today, weekends, Mondays, empty days and relative weight', () => {
    const counts = new Map([
      ['2026-11-16', 4],
      ['2026-11-14', 2],
    ])
    const [tue, mon, sun, sat] = daysOf(
      buildDayIndex('2026-11-14', '2026-11-17', counts, '2026-11-17')
    )

    expect(tue).toMatchObject({ isToday: true, count: 0, weight: 0, dayOfMonth: 17 })
    expect(mon).toMatchObject({ isMonday: true, isWeekend: false, count: 4, weight: 1 })
    expect(sun).toMatchObject({ isWeekend: true, count: 0 })
    expect(sat).toMatchObject({ isWeekend: true, isToday: false, weight: 0.5 })
  })

  it('is empty for a reversed range', () => {
    expect(buildDayIndex('2026-11-17', '2026-11-16', new Map(), '2026-11-17')).toEqual([])
  })
})

describe('nearestDayWithEntries', () => {
  const days = daysOf(
    buildDayIndex(
      '2026-11-10',
      '2026-11-16',
      new Map([
        ['2026-11-16', 1],
        ['2026-11-12', 1],
      ]),
      '2026-11-16'
    )
  )

  it('keeps a day that has events', () => {
    expect(nearestDayWithEntries(days, 4)).toBe(4)
  })

  it('moves to the closest day with events, newer first on a tie', () => {
    expect(nearestDayWithEntries(days, 1)).toBe(0)
    expect(nearestDayWithEntries(days, 2)).toBe(0)
    expect(nearestDayWithEntries(days, 3)).toBe(4)
    expect(nearestDayWithEntries(days, 6)).toBe(4)
  })

  it('finds nothing without events', () => {
    expect(
      nearestDayWithEntries(daysOf(buildDayIndex('2026-11-10', '2026-11-12', new Map(), '')), 1)
    ).toBe(-1)
  })
})

describe('densityFor', () => {
  it('switches at 17 and 6 pixels per day', () => {
    expect(densityFor(17)).toBe('full')
    expect(densityFor(16.9)).toBe('sparse')
    expect(densityFor(6)).toBe('sparse')
    expect(densityFor(5.9)).toBe('dense')
  })
})

describe('slotAt', () => {
  const centres = [10, 20, 30, 40]

  it('picks the closest centre, the upper one on a tie', () => {
    expect(slotAt(centres, 21)).toBe(1)
    expect(slotAt(centres, 26)).toBe(2)
    expect(slotAt(centres, 25)).toBe(1)
  })

  it('clamps outside the track', () => {
    expect(slotAt(centres, -50)).toBe(0)
    expect(slotAt(centres, 500)).toBe(3)
    expect(slotAt([], 5)).toBe(-1)
  })
})
