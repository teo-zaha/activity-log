import { describe, expect, it } from 'vitest'
import { BackendFailure } from '../../services/backend/backend-failure'
import { failureOf } from '../../services/failure'
import type { RangeCoverage } from '../../services/activity-selection.service'
import type { CollectionStatus } from '../../services/types'
import type { DayKey } from '../../utils/local-day'
import { sharedEmptyText } from '../empty-state/empty-reason'
import { calendarEmptyHint, calendarHintText, type CalendarEmptyInput } from './calendar-empty-hint'

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

const lostBackend: CollectionStatus = {
  kind: 'failed',
  operation: 'collect',
  origin: 'lostBackend',
  failure: failureOf(new BackendFailure('unreachable', 'gone')),
}

const hint = (input: Partial<CalendarEmptyInput>): ReturnType<typeof calendarEmptyHint> =>
  calendarEmptyHint({
    unit: 'week',
    days: WEEK,
    ...OWN,
    coverage: { kind: 'covered' },
    loading: false,
    status: { kind: 'idle' },
    hasSources: true,
    anyShown: true,
    ...input,
  })

describe('calendarEmptyHint', () => {
  it('says a collected week or day had no activity, and offers nothing', () => {
    expect(hint({})).toEqual({ reason: 'noActivity', unit: 'week', whole: true })
    expect(hint({ unit: 'day', days: ['2026-11-17'] })).toEqual({
      reason: 'noActivity',
      unit: 'day',
      whole: true,
    })
  })

  it('says only the collected days had no activity where the week reaches past them', () => {
    expect(hint({ from: '2026-11-22', to: '2026-11-29' })).toEqual({
      reason: 'noActivity',
      unit: 'week',
      whole: false,
    })
  })

  it('says a week outside this tab’s range was not collected and names the days on screen', () => {
    const days = WEEK.map(day => day.replace('2026-11-', '2026-12-') as DayKey)

    // Collecting this tab's range would not bring these days.
    expect(hint({ days })).toEqual({
      reason: 'notCollected',
      unit: 'week',
      onScreen: OWN,
      collect: null,
    })
  })

  it('offers to collect this tab’s range where another collection on screen leaves it out', () => {
    expect(hint({ coverage: other('2026-11-02', '2026-11-08', false) })).toEqual({
      reason: 'notCollected',
      unit: 'week',
      onScreen: { from: '2026-11-02', to: '2026-11-08' },
      collect: OWN,
    })
  })

  it('says the rest of this tab’s range is not collected where the collection covers part', () => {
    expect(hint({ coverage: other('2026-11-10', '2026-11-17', true) })).toEqual({
      reason: 'partlyCollected',
      collected: { from: '2026-11-10', to: '2026-11-17' },
      own: OWN,
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
      reason: 'noActivity',
      unit: 'week',
      whole: true,
    })
  })

  it('says there was no activity without a range to compare', () => {
    expect(hint({ from: null, to: null, coverage: { kind: 'unknown' } })).toEqual({
      reason: 'noActivity',
      unit: 'week',
      whole: true,
    })
  })

  it('says nothing while a collection runs', () => {
    expect(hint({ loading: true })).toBeNull()
    expect(hint({ loading: true, coverage: other('2026-11-02', '2026-11-08', false) })).toBeNull()
  })

  it('names a failed collection before any range, and offers no Collect beside its Retry', () => {
    expect(hint({ status: lostBackend })).toEqual({ reason: 'failed', cause: 'unfinished' })
    expect(
      hint({ status: lostBackend, coverage: other('2026-11-02', '2026-11-08', false) })
    ).toEqual({ reason: 'failed', cause: 'unfinished' })
  })

  it('says every source is hidden, or none is ready, before any range', () => {
    const coverage = other('2026-11-02', '2026-11-08', false)

    expect(hint({ anyShown: false, coverage })).toEqual({ reason: 'allHidden' })
    expect(hint({ hasSources: false, anyShown: false, coverage })).toEqual({
      reason: 'noSources',
    })
  })
})

describe('calendarHintText', () => {
  it('words the calendar’s own reasons', () => {
    expect(calendarHintText({ reason: 'noActivity', unit: 'week', whole: true })).toEqual({
      title: 'No activity in this week',
      message: null,
      action: null,
    })
    expect(calendarHintText({ reason: 'noActivity', unit: 'day', whole: true }).title).toBe(
      'No activity on this day'
    )
    expect(calendarHintText({ reason: 'noActivity', unit: 'week', whole: false }).title).toBe(
      'No activity in the collected days'
    )
    expect(
      calendarHintText({
        reason: 'notCollected',
        unit: 'week',
        onScreen: { from: '2026-11-02', to: '2026-11-08' },
        collect: OWN,
      })
    ).toEqual({
      title: 'This week was not collected',
      message: 'The activity on screen covers 2 Nov – 8 Nov 2026.',
      action: { kind: 'collect-own-range', label: 'Collect 16 Nov – 22 Nov 2026' },
    })
    expect(
      calendarHintText({ reason: 'notCollected', unit: 'day', onScreen: null, collect: null })
    ).toEqual({ title: 'This day was not collected', message: null, action: null })
  })

  it('words the reasons it shares with the Timeline as the Timeline does', () => {
    const partly = {
      reason: 'partlyCollected',
      collected: { from: '2026-11-10', to: '2026-11-17' },
      own: OWN,
    } as const

    expect(calendarHintText(partly)).toEqual(sharedEmptyText(partly))
    expect(calendarHintText({ reason: 'allHidden' })).toEqual(
      sharedEmptyText({ reason: 'allHidden' })
    )
  })
})
