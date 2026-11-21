import { describe, expect, it } from 'vitest'
import type { RangeCoverage } from '../../services/activity-selection.service'
import type { DayKey } from '../../utils/local-day'
import { collectedDays } from './collected-days'

const WEEK: DayKey[] = ['2026-11-16', '2026-11-17', '2026-11-18', '2026-11-19', '2026-11-20']

describe('collectedDays', () => {
  it('keeps the days of this tab’s range', () => {
    const covered: RangeCoverage = { kind: 'covered' }

    expect(collectedDays(WEEK, '2026-11-18', '2026-11-30', covered)).toEqual([
      '2026-11-18',
      '2026-11-19',
      '2026-11-20',
    ])
    expect(collectedDays(WEEK, '2026-11-01', '2026-11-15', covered)).toEqual([])
  })

  it('keeps every day without a range to compare', () => {
    expect(collectedDays(WEEK, null, null, { kind: 'unknown' })).toEqual(WEEK)
  })

  it('keeps only the days another collection on screen holds too', () => {
    const other = (from: DayKey, to: DayKey): RangeCoverage => ({
      kind: 'other',
      collected: { from, to },
      own: { from: '2026-11-16', to: '2026-11-22' },
      overlaps: true,
    })

    expect(
      collectedDays(WEEK, '2026-11-16', '2026-11-22', other('2026-11-10', '2026-11-17'))
    ).toEqual(['2026-11-16', '2026-11-17'])
    expect(
      collectedDays(WEEK, '2026-11-16', '2026-11-22', other('2026-11-01', '2026-11-05'))
    ).toEqual([])
  })
})
