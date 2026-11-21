import type { RangeCoverage } from '../../services/activity-selection.service'
import type { DayRange } from '../../services/activity-view-state.service'
import { keysWithin, type DayKey } from '../../utils/local-day'
import {
  collectOwnRange,
  coversText,
  failedReason,
  partlyCollected,
  sharedEmptyText,
  sourcesReason,
  type CollectOwnRange,
  type EmptyText,
  type SharedEmptyInput,
  type SharedEmptyReason,
} from '../empty-state/empty-reason'
import { collectedDays } from './collected-days'

/** What a calendar view shows: one day, or a (work) week. */
export type CalendarUnit = 'day' | 'week'

/** Why an empty calendar view is empty. */
export type CalendarEmptyHint =
  | SharedEmptyReason
  /** The view's collected days hold nothing; `whole` where that is every day it shows. */
  | { readonly reason: 'noActivity'; readonly unit: CalendarUnit; readonly whole: boolean }
  /**
   * None of the view's days were collected. `onScreen` is what the activity on screen covers;
   * `collect` is this tab's range where collecting it would bring days of the view.
   */
  | {
      readonly reason: 'notCollected'
      readonly unit: CalendarUnit
      readonly onScreen: DayRange | null
      readonly collect: DayRange | null
    }

export interface CalendarEmptyInput extends SharedEmptyInput {
  readonly unit: CalendarUnit
  readonly days: readonly DayKey[]
  /** This tab's range, the collected one where it has none (`ActivityViewStateService`). */
  readonly from: DayKey | null
  readonly to: DayKey | null
  readonly coverage: RangeCoverage
  /** A collection runs: what it brings may yet fill the view. */
  readonly loading: boolean
}

/**
 * The hint of a calendar view without entries; null while a collection runs. A failed
 * collection, no ready source or every source hidden come first, as in the Timeline. A view none
 * of whose days were collected says so and names the days the activity on screen covers; where
 * collecting this tab's own range would bring days of the view (another collection is on
 * screen), it offers that.
 */
export function calendarEmptyHint(input: CalendarEmptyInput): CalendarEmptyHint | null {
  if (input.loading) return null
  const shared = failedReason(input.status) ?? sourcesReason(input)
  if (shared) return shared

  const { unit, days, from, to, coverage } = input
  const collected = collectedDays(days, from, to, coverage)
  const missing = coverage.kind === 'other' ? coverage : null
  const bringsDays = missing !== null && keysWithin(days, from, to).length > collected.length

  if (collected.length > 0) {
    // Collected days of the view, and others it would collect: part of this tab's range is on screen.
    const partly = bringsDays ? partlyCollected(coverage) : null
    return partly ?? { reason: 'noActivity', unit, whole: collected.length === days.length }
  }
  return {
    reason: 'notCollected',
    unit,
    onScreen: missing?.collected ?? (from !== null && to !== null ? { from, to } : null),
    collect: bringsDays ? missing.own : null,
  }
}

/** The words and action of a calendar view's hint. */
export function calendarHintText(hint: CalendarEmptyHint): EmptyText<CollectOwnRange> {
  switch (hint.reason) {
    case 'noActivity':
      return {
        title: !hint.whole
          ? 'No activity in the collected days'
          : hint.unit === 'day'
            ? 'No activity on this day'
            : 'No activity in this week',
        message: null,
        action: null,
      }
    case 'notCollected':
      return {
        title: `This ${hint.unit} was not collected`,
        message: hint.onScreen ? `${coversText(hint.onScreen)}.` : null,
        action: hint.collect ? collectOwnRange(hint.collect) : null,
      }
    default:
      return sharedEmptyText(hint)
  }
}
