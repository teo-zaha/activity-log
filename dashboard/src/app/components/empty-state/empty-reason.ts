import type { RangeCoverage } from '../../services/activity-selection.service'
import type { DayRange } from '../../services/activity-view-state.service'
import { refusedRange, retryable } from '../../services/collection-failure'
import type { CollectionStatus } from '../../services/types'
import { dayRangeLabel } from '../../utils/local-day'

/** Why a failed collection left nothing to show, see `services/collection-failure.ts`. */
export type FailedCollectionCause = 'refusedRange' | 'refusedRead' | 'unfinished'

/**
 * Why a view shows no activity, as the Timeline and the calendar views both tell it. Each view
 * adds its own reasons and decides between them over what it shows (`timelineEmptyState`,
 * `calendarEmptyHint`); the text is written once, here, for these.
 */
export type SharedEmptyReason =
  | { readonly reason: 'failed'; readonly cause: FailedCollectionCause }
  /** No source is usable or still being checked. */
  | { readonly reason: 'noSources' }
  /** The source filter shows no connection. */
  | { readonly reason: 'allHidden' }
  /** Another collection on screen holds part of this tab's range, and nothing in it. */
  | { readonly reason: 'partlyCollected'; readonly collected: DayRange; readonly own: DayRange }

/** Collect this tab's own range (`ActivityDataService.collectShownRange`). */
export interface CollectOwnRange {
  readonly kind: 'collect-own-range'
  readonly label: string
}

/** What an empty view offers to do about it. */
export type EmptyAction = { readonly kind: 'clear-search' } | CollectOwnRange

/** An empty view's words and action, as its template shows them. */
export interface EmptyText<Action extends EmptyAction = EmptyAction> {
  readonly title: string
  readonly message: string | null
  readonly action: Action | null
}

/** What the views read to tell their shared reasons apart. */
export interface SharedEmptyInput {
  readonly status: CollectionStatus
  /** Whether any source is usable or still being checked. */
  readonly hasSources: boolean
  /** Whether the source filter shows any connection. */
  readonly anyShown: boolean
}

/** The failed collection's reason, where the collection on screen failed. */
export function failedReason(status: CollectionStatus): SharedEmptyReason | null {
  if (status.kind !== 'failed') return null
  const cause: FailedCollectionCause = refusedRange(status)
    ? 'refusedRange'
    : retryable(status)
      ? 'unfinished'
      : 'refusedRead'
  return { reason: 'failed', cause }
}

/** No source to show: none ready, or every one hidden by the source filter. */
export function sourcesReason(input: SharedEmptyInput): SharedEmptyReason | null {
  if (!input.hasSources) return { reason: 'noSources' }
  if (!input.anyShown) return { reason: 'allHidden' }
  return null
}

/** Another collection on screen, which holds part of this tab's range: what it leaves out. */
export function partlyCollected(coverage: RangeCoverage): SharedEmptyReason | null {
  return coverage.kind === 'other' && coverage.overlaps
    ? { reason: 'partlyCollected', collected: coverage.collected, own: coverage.own }
    : null
}

/** `Collect 10 May – 17 May 2026`: collects this tab's own range. */
export function collectOwnRange(own: DayRange): CollectOwnRange {
  return { kind: 'collect-own-range', label: `Collect ${dayRangeLabel(own.from, own.to)}` }
}

/** `The activity on screen covers 1 May – 7 May 2026` */
export function coversText(collected: DayRange): string {
  return `The activity on screen covers ${dayRangeLabel(collected.from, collected.to)}`
}

const FAILED_MESSAGE: Readonly<Record<FailedCollectionCause, string>> = {
  refusedRange: 'The backend refused to collect this range. Choose another one.',
  refusedRead: 'The backend refused to show its activity; check source status in Settings.',
  unfinished:
    'The last collection did not finish. Retry above, or check source status in Settings.',
}

/** The words of a shared reason. */
export function sharedEmptyText(reason: SharedEmptyReason): EmptyText<CollectOwnRange> {
  switch (reason.reason) {
    case 'failed':
      return { title: 'No activity loaded', message: FAILED_MESSAGE[reason.cause], action: null }
    case 'noSources':
      return {
        title: 'No sources are ready',
        message: 'Open Settings to see why each source is unavailable and configure it.',
        action: null,
      }
    case 'allHidden':
      return {
        title: 'All sources are hidden',
        message: 'Turn on at least one source in the sidebar to see its activity.',
        action: null,
      }
    case 'partlyCollected':
      return {
        title: 'Nothing recorded in the collected days',
        message: `${coversText(reason.collected)}; the rest of ${dayRangeLabel(reason.own.from, reason.own.to)} is not collected.`,
        action: collectOwnRange(reason.own),
      }
  }
}
