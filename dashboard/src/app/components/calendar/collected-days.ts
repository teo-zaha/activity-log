import type { RangeCoverage } from '../../services/activity-selection.service'
import { keysWithin, type DayKey } from '../../utils/local-day'

/**
 * The days of `days` whose entries the views show: within this tab's range (`from`–`to`, the
 * collected range where the tab has none) and, while another collection is on screen, within
 * that one too. The others were not read: no meeting time is totalled for them, and a view of
 * none of them says it was not collected rather than that nothing happened.
 */
export function collectedDays(
  days: readonly DayKey[],
  from: DayKey | null,
  to: DayKey | null,
  coverage: RangeCoverage
): DayKey[] {
  const own = keysWithin(days, from, to)
  return coverage.kind === 'other'
    ? keysWithin(own, coverage.collected.from, coverage.collected.to)
    : own
}
