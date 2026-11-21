import { describe, expect, it } from 'vitest'
import type { RangeCoverage } from '../../services/activity-selection.service'
import type { DayKey } from '../../utils/local-day'
import { calendarEmptyHint, type CalendarEmptyInput } from './calendar-empty-hint'

const WEEK: DayKey[] = [
  '2026-11-16',
  '2026-11-17',
  '2026-11-18',
  '2026-11-19',
  '2026-11-20',
  '2026-11-21',
  '2026-11-22',
]
const OWN = { from: '2026-11-16', to: '2026-11-22' } as const

const other = (from: DayKey, to: DayKey, overlaps: boolean): RangeCoverage => ({
  kind: 'other',
  collected: { from, to },
  own: OWN,
  overlaps,
})

const hint = (input: Partial<CalendarEmptyInput>): ReturnType<typeof calendarEmptyHint> =>
  calendarEmptyHint({ unit: 'week', days: WEEK, ...OWN, coverage: { kind: 'covered' }, ...input })

describe('calendarEmptyHint', () => {
  it('says a collected week had no activity, and offers nothing', () => {
    expect(hint({})).toEqual({ title: 'No activity in this week', message: null, action: null })
    expect(hint({ unit: 'day', days: ['2026-11-17'] }).title).toBe('No activity on this day')
  })

  it('says a week outside this tab’s range was not collected and names the days on screen', () => {
    const days = WEEK.map(day => day.replace('2026-11-', '2026-12-') as DayKey)

    expect(hint({ days })).toEqual({
      title: 'This week was not collected',
      message: 'The activity on screen covers 16 Nov – 22 Nov 2026.',
      // Collecting this tab's range would not bring these days.
      action: null,
    })
  })

  it('offers to collect this tab’s range where another collection on screen leaves it out', () => {
    expect(hint({ coverage: other('2026-11-02', '2026-11-08', false) })).toEqual({
      title: 'This week was not collected',
      message: 'The activity on screen covers 2 Nov – 8 Nov 2026.',
      action: { kind: 'collect-own-range', label: 'Collect 16 Nov – 22 Nov 2026' },
    })
  })

  it('says the rest of this tab’s range is not collected where the collection covers part', () => {
    expect(hint({ coverage: other('2026-11-10', '2026-11-17', true) })).toEqual({
      title: 'Nothing recorded in the collected days',
      message:
        'The activity on screen covers 10 Nov – 17 Nov 2026; the rest of 16 Nov – 22 Nov 2026 is not collected.',
      action: { kind: 'collect-own-range', label: 'Collect 16 Nov – 22 Nov 2026' },
    })
  })

  it('offers nothing where the days of the view this tab would collect are all on screen', () => {
    // The view's days are collected; this tab's range reaches further, but not into the view.
    const coverage: RangeCoverage = {
      kind: 'other',
      collected: { from: '2026-11-16', to: '2026-11-22' },
      own: { from: '2026-11-16', to: '2026-11-30' },
      overlaps: true,
    }

    expect(hint({ to: '2026-11-30', coverage })).toEqual({
      title: 'No activity in this week',
      message: null,
      action: null,
    })
  })

  it('says there was no activity without a range to compare', () => {
    expect(hint({ from: null, to: null, coverage: { kind: 'unknown' } }).title).toBe(
      'No activity in this week'
    )
  })
})
