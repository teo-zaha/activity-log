import type { RangeCoverage } from '../../services/activity-selection.service'
import { dayRangeLabel, keysWithin, type DayKey } from '../../utils/local-day'
import { collectedDays } from './collected-days'

/** Collect this tab's own range, which the collection on screen leaves out. */
export interface CollectOwnRange {
  readonly kind: 'collect-own-range'
  readonly label: string
}

/** Why an empty calendar view is empty, and what to do about it. */
export interface CalendarEmptyHint {
  readonly title: string
  /** The days the activity on screen covers, where that explains the view; else null. */
  readonly message: string | null
  readonly action: CollectOwnRange | null
}

export interface CalendarEmptyInput {
  /** What the view shows: one day, or a (work) week. */
  readonly unit: 'day' | 'week'
  readonly days: readonly DayKey[]
  /** This tab's range, the collected one where it has none (`ActivityViewStateService`). */
  readonly from: DayKey | null
  readonly to: DayKey | null
  readonly coverage: RangeCoverage
}

/**
 * The hint of a calendar view without entries. A view none of whose days were collected says so
 * and names the days the activity on screen covers; where collecting this tab's own range would
 * bring days of the view (another collection is on screen), it offers that.
 */
export function calendarEmptyHint(input: CalendarEmptyInput): CalendarEmptyHint {
  const { unit, days, from, to, coverage } = input
  const collected = collectedDays(days, from, to, coverage)
  const missing = coverage.kind === 'other' ? coverage : null
  const action: CollectOwnRange | null =
    missing !== null && keysWithin(days, from, to).length > collected.length
      ? {
          kind: 'collect-own-range',
          label: `Collect ${dayRangeLabel(missing.own.from, missing.own.to)}`,
        }
      : null

  if (collected.length > 0) {
    if (missing !== null && action !== null) {
      const covers = dayRangeLabel(missing.collected.from, missing.collected.to)
      const own = dayRangeLabel(missing.own.from, missing.own.to)
      return {
        title: 'Nothing recorded in the collected days',
        message: `The activity on screen covers ${covers}; the rest of ${own} is not collected.`,
        action,
      }
    }
    return {
      title: unit === 'day' ? 'No activity on this day' : 'No activity in this week',
      message: null,
      action: null,
    }
  }
  const onScreen = missing?.collected ?? (from !== null && to !== null ? { from, to } : null)
  return {
    title: `This ${unit} was not collected`,
    message: onScreen
      ? `The activity on screen covers ${dayRangeLabel(onScreen.from, onScreen.to)}.`
      : null,
    action,
  }
}
