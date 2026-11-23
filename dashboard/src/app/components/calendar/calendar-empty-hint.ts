import type { RangeCoverage } from '../../services/activity-selection.service'
import type { DayRange } from '../../services/activity-view-state.service'
import { collectableRange } from '../../utils/app-url'
import { keysWithin, type DayKey } from '../../utils/local-day'
import {
  collectRange,
  coversText,
  failedReason,
  partlyCollected,
  sharedEmptyText,
  sourcesReason,
  type CollectRange,
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
  /**
   * The view's collected days hold nothing. `collect` is null where those are every day it shows;
   * otherwise the range that takes the rest in (`rangeTakingIn`).
   */
  | {
      readonly reason: 'noActivity'
      readonly unit: CalendarUnit
      readonly collect: DayRange | null
    }
  /**
   * None of the view's days were collected. `onScreen` is what the activity on screen covers;
   * `collect` is the range that takes the view's days in (`rangeTakingIn`).
   */
  | {
      readonly reason: 'notCollected'
      readonly unit: CalendarUnit
      readonly onScreen: DayRange | null
      readonly collect: DayRange
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
 * of whose days were collected says so and names the days the activity on screen covers. A hint
 * offers one Collect: where another collection on screen leaves out days of the view that this
 * tab's range holds, a view partly collected offers this tab's range (`partlyCollected`);
 * otherwise a view with days not collected offers the range that takes them in.
 */
export function calendarEmptyHint(input: CalendarEmptyInput): CalendarEmptyHint | null {
  if (input.loading) return null
  const shared = failedReason(input.status) ?? sourcesReason(input)
  if (shared) return shared

  const { unit, days, from, to, coverage } = input
  const collected = collectedDays(days, from, to, coverage)
  const missing = coverage.kind === 'other' ? coverage : null
  const bringsDays = missing !== null && keysWithin(days, from, to).length > collected.length

  const own = from !== null && to !== null ? { from, to } : null

  if (collected.length > 0) {
    // Collected days of the view, and others it would collect: part of this tab's range is on screen.
    const partly = bringsDays ? partlyCollected(coverage) : null
    const whole = collected.length === days.length
    return (
      partly ?? { reason: 'noActivity', unit, collect: whole ? null : rangeTakingIn(own, days) }
    )
  }
  return {
    reason: 'notCollected',
    unit,
    onScreen: missing?.collected ?? own,
    collect: rangeTakingIn(own, days),
  }
}

/**
 * This tab's range taken out to the view's days, so that collecting it brings them: one span
 * from the earlier start to the later end. Only the view's days where that span is longer than
 * the backend collects (`collectableRange`), or without a range.
 */
export function rangeTakingIn(own: DayRange | null, days: readonly DayKey[]): DayRange {
  const view = { from: days[0], to: days[days.length - 1] }
  if (own === null) return view
  const span = {
    from: own.from < view.from ? own.from : view.from,
    to: own.to > view.to ? own.to : view.to,
  }
  return collectableRange(span.from, span.to) ? span : view
}

/** The words and action of a calendar view's hint. */
export function calendarHintText(hint: CalendarEmptyHint): EmptyText<CollectRange> {
  switch (hint.reason) {
    case 'noActivity':
      return {
        title: hint.collect
          ? 'No activity in the collected days'
          : hint.unit === 'day'
            ? 'No activity on this day'
            : 'No activity in this week',
        message: null,
        action: hint.collect ? collectRange(hint.collect) : null,
      }
    case 'notCollected':
      return {
        title: `This ${hint.unit} was not collected`,
        message: hint.onScreen ? `${coversText(hint.onScreen)}.` : null,
        action: collectRange(hint.collect),
      }
    default:
      return sharedEmptyText(hint)
  }
}
