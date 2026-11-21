import type { RangeCoverage } from '../../services/activity-selection.service'
import type { DayRange } from '../../services/activity-view-state.service'
import { dayRangeLabel } from '../../utils/local-day'
import {
  collectOwnRange,
  coversText,
  failedReason,
  partlyCollected,
  sharedEmptyText,
  sourcesReason,
  type EmptyText,
  type SharedEmptyInput,
  type SharedEmptyReason,
} from '../empty-state/empty-reason'

/** What decides why the Timeline's list is empty. */
export interface TimelineEmptyInput extends SharedEmptyInput {
  /** The filter, trimmed. */
  readonly query: string
  /** How the collection on screen covers this tab's range. */
  readonly coverage: RangeCoverage
}

/** Why the Timeline's list is empty. */
export type TimelineEmptyState =
  | SharedEmptyReason
  | { readonly reason: 'noMatches'; readonly query: string }
  /** Another collection on screen holds none of this tab's range. */
  | { readonly reason: 'notCollected'; readonly collected: DayRange; readonly own: DayRange }
  /** This tab's range is collected and holds nothing. */
  | { readonly reason: 'nothingRecorded' }

/** Why the Timeline's list is empty, and what to do about it. */
export function timelineEmptyState(input: TimelineEmptyInput): TimelineEmptyState {
  const { query, coverage } = input
  const missing = coverage.kind === 'other' ? coverage : null
  const failed = failedReason(input.status)
  if (failed) return failed
  // A filter explains an empty list only where some of this tab's days are collected.
  if (query && (missing === null || missing.overlaps)) return { reason: 'noMatches', query }
  const sources = sourcesReason(input)
  if (sources) return sources
  if (missing && !missing.overlaps) {
    return { reason: 'notCollected', collected: missing.collected, own: missing.own }
  }
  return partlyCollected(coverage) ?? { reason: 'nothingRecorded' }
}

/** The empty state as the Timeline shows it: an icon and its words. */
export interface TimelineEmptyView extends EmptyText {
  readonly icon: string
}

const ICONS: Readonly<Record<TimelineEmptyState['reason'], string>> = {
  failed: 'cloud_off',
  noMatches: 'search_off',
  noSources: 'power_off',
  allHidden: 'filter_alt_off',
  notCollected: 'date_range',
  partlyCollected: 'date_range',
  nothingRecorded: 'event_busy',
}

function timelineText(state: TimelineEmptyState): EmptyText {
  switch (state.reason) {
    case 'noMatches':
      return {
        title: `No matches for “${state.query}”`,
        message: 'Try a different term, or filter by repo, branch, ticket key or author.',
        action: { kind: 'clear-search' },
      }
    case 'notCollected':
      return {
        title: 'This range is not collected',
        message: `${coversText(state.collected)}, not ${dayRangeLabel(state.own.from, state.own.to)}.`,
        action: collectOwnRange(state.own),
      }
    case 'nothingRecorded':
      return {
        title: 'Nothing recorded in this range',
        message: 'Pick a wider date range and collect again.',
        action: null,
      }
    default:
      return sharedEmptyText(state)
  }
}

/** The icon and words of the Timeline's empty state. */
export function timelineEmptyView(state: TimelineEmptyState): TimelineEmptyView {
  return { icon: ICONS[state.reason], ...timelineText(state) }
}
