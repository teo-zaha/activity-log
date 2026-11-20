import { describe, expect, it } from 'vitest'
import type { DayKey } from './local-day'
import type { DayIndexDay, DayIndexItem } from '../models/day-index'
import {
  buildDayIndex,
  dayIndexOf,
  densityFor,
  isMonthDensity,
  monthDensityFor,
  monthRowLabel,
  monthRows,
  nearestDayWithEntries,
  slotAt,
} from './day-index'

function daysOf(items: readonly DayIndexItem[]): DayIndexDay[] {
  return items.filter((item): item is DayIndexDay => item.kind === 'day')
}

describe('buildDayIndex', () => {
  it('lists every day newest first with a heading per month and the year where it changes', () => {
    const items = buildDayIndex('2025-11-29', '2026-01-02', new Map(), '2026-01-02')

    expect(items.filter(item => item.kind === 'month').map(item => item.label)).toEqual([
      'Jan',
      'Dec ’25',
      'Nov ’25',
    ])
    expect(items.slice(0, 3).map(item => item.key)).toEqual(['2026-01', '2026-01-02', '2026-01-01'])
  })

  it('gives the oldest heading its year only when the range spans years', () => {
    const labels = (from: DayKey, to: DayKey): string[] =>
      buildDayIndex(from, to, new Map(), to).flatMap(item =>
        item.kind === 'month' ? [item.label] : []
      )

    expect(labels('2025-12-30', '2026-01-02')).toEqual(['Jan', 'Dec ’25'])
    expect(labels('2026-10-30', '2026-11-02')).toEqual(['Nov', 'Oct'])
    expect(labels('2026-11-01', '2026-11-02')).toEqual(['Nov'])
  })

  it('cuts every month label to three letters', () => {
    const [heading] = buildDayIndex('2026-09-01', '2026-09-01', new Map(), '2026-09-01')

    expect(heading).toEqual({ kind: 'month', key: '2026-09', day: '2026-09-01', label: 'Sep' })
  })

  it('marks today, weekends, Mondays, empty days and relative weight', () => {
    const counts = new Map<DayKey, number>([
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

describe('dayIndexOf', () => {
  it('spans from the oldest to the newest day with events, keeping the empty days between', () => {
    const items = dayIndexOf(
      new Map([
        ['2026-11-12', 2],
        ['2026-11-09', 1],
        ['2026-11-01', 0],
      ]),
      '2026-11-17'
    )

    expect(daysOf(items).map(day => day.key)).toEqual([
      '2026-11-12',
      '2026-11-11',
      '2026-11-10',
      '2026-11-09',
    ])
    // Today lies outside the span: no day is marked.
    expect(daysOf(items).some(day => day.isToday)).toBe(false)
  })

  it('labels the ends with their months and years', () => {
    const items = dayIndexOf(
      new Map([
        ['2026-01-02', 1],
        ['2025-12-30', 1],
      ]),
      '2026-11-17'
    )

    expect(items.flatMap(item => (item.kind === 'month' ? [item.label] : []))).toEqual([
      'Jan',
      'Dec ’25',
    ])
  })

  it('is empty for fewer than two days with events', () => {
    expect(dayIndexOf(new Map([['2026-11-12', 3]]), '2026-11-17')).toEqual([])
    expect(dayIndexOf(new Map(), '2026-11-17')).toEqual([])
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
      nearestDayWithEntries(
        daysOf(buildDayIndex('2026-11-10', '2026-11-12', new Map(), '2026-11-20')),
        1
      )
    ).toBe(-1)
  })
})

describe('densityFor', () => {
  it('switches at 17, 6 and 3 pixels per day', () => {
    expect(densityFor(17)).toBe('full')
    expect(densityFor(16.9)).toBe('sparse')
    expect(densityFor(6)).toBe('sparse')
    expect(densityFor(5.9)).toBe('dense')
    expect(densityFor(3)).toBe('dense')
    expect(densityFor(2.9)).toBe('months')
  })
})

describe('monthRows', () => {
  it('sums each month in the range and marks the year on the newest month of an older year', () => {
    const items = buildDayIndex(
      '2025-11-20',
      '2026-01-05',
      new Map([
        ['2026-01-02', 4],
        ['2025-12-24', 1],
        ['2025-12-01', 1],
        ['2025-11-20', 8],
      ]),
      '2026-01-05'
    )

    expect(monthRows(items)).toEqual([
      { key: '2026-01', label: 'Jan', year: null, days: 5, count: 4, weight: 0.5 },
      { key: '2025-12', label: 'Dec', year: '’25', days: 31, count: 2, weight: 0.25 },
      { key: '2025-11', label: 'Nov', year: null, days: 11, count: 8, weight: 1 },
    ])
  })
})

describe('monthDensityFor', () => {
  it('switches at 22, 13 and 7 pixels per month', () => {
    expect(monthDensityFor(22)).toBe('months')
    expect(monthDensityFor(21.9)).toBe('months-compact')
    expect(monthDensityFor(13)).toBe('months-compact')
    expect(monthDensityFor(12.9)).toBe('quarters')
    expect(monthDensityFor(7)).toBe('quarters')
    expect(monthDensityFor(6.9)).toBe('years')
  })

  it('tells month densities from day densities', () => {
    expect(isMonthDensity('dense')).toBe(false)
    expect(isMonthDensity('quarters')).toBe(true)
  })
})

describe('monthRowLabel', () => {
  // Feb 2026 back to Oct 2025.
  const rows = monthRows(buildDayIndex('2025-10-01', '2026-02-28', new Map(), '2026-02-28'))
  const labels = (density: Parameters<typeof monthRowLabel>[1]): (string | null)[] =>
    rows.map(row => monthRowLabel(row, density))

  it('names every month while rows are tall enough', () => {
    expect(labels('months')).toEqual(['Feb', 'Jan', 'Dec', 'Nov', 'Oct'])
  })

  it('puts the short year on January when rows are thin', () => {
    expect(labels('months-compact')).toEqual(['Feb', '’26', 'Dec', 'Nov', 'Oct'])
  })

  it('labels quarters and years, leaving ticks between them', () => {
    expect(labels('quarters')).toEqual([null, '2026', null, null, 'Oct'])
    expect(labels('years')).toEqual([null, '2026', null, null, null])
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
