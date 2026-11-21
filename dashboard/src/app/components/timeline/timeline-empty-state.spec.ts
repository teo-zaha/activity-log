import { describe, expect, it } from 'vitest'
import { BackendFailure } from '../../services/backend/backend-failure'
import { failureOf } from '../../services/failure'
import type { CollectionStatus, FailureOrigin } from '../../services/types'
import type { RangeCoverage } from '../../services/activity-selection.service'
import {
  timelineEmptyState,
  timelineEmptyView,
  type TimelineEmptyInput,
  type TimelineEmptyState,
} from './timeline-empty-state'

const failed = (
  origin: FailureOrigin,
  kind: 'invalidRequest' | 'unreachable'
): CollectionStatus => ({
  kind: 'failed',
  operation: 'collect',
  origin,
  failure: failureOf(new BackendFailure(kind, 'refused')),
})

const base: TimelineEmptyInput = {
  query: '',
  status: { kind: 'idle' },
  hasSources: true,
  anyShown: true,
  coverage: { kind: 'covered' },
}

const COLLECTED = { from: '2026-05-01', to: '2026-05-07' } as const
const OWN_APART = { from: '2026-05-10', to: '2026-05-17' } as const
const OWN_OVERLAPPING = { from: '2026-05-05', to: '2026-05-17' } as const

/** The collection on screen covers 1–7 May; this tab's range is 10–17 May, or 5–17 May. */
const other = (overlaps: boolean): RangeCoverage => ({
  kind: 'other',
  collected: COLLECTED,
  own: overlaps ? OWN_OVERLAPPING : OWN_APART,
  overlaps,
})

describe('timelineEmptyState', () => {
  it.each<[string, Partial<TimelineEmptyInput>, TimelineEmptyState]>([
    [
      'a collect refused as invalid asks for another range',
      { status: failed('collect', 'invalidRequest'), query: 'x' },
      { reason: 'failed', cause: 'refusedRange' },
    ],
    [
      'a refused read offers no retry',
      { status: failed('refusedRead', 'invalidRequest') },
      { reason: 'failed', cause: 'refusedRead' },
    ],
    [
      'a lost backend points at Retry',
      { status: failed('lostBackend', 'unreachable') },
      { reason: 'failed', cause: 'unfinished' },
    ],
    [
      'a filter that matches nothing offers to clear it',
      { query: 'nope', hasSources: false },
      { reason: 'noMatches', query: 'nope' },
    ],
    [
      'no sources sends to Settings',
      { hasSources: false, anyShown: false },
      { reason: 'noSources' },
    ],
    ['every source hidden sends to the sidebar', { anyShown: false }, { reason: 'allHidden' }],
    ['an empty range asks for a wider one', {}, { reason: 'nothingRecorded' }],
    [
      'without a range to compare, an empty range asks for a wider one',
      { coverage: { kind: 'unknown' } },
      { reason: 'nothingRecorded' },
    ],
    [
      'a range the collection leaves out names both',
      { coverage: other(false) },
      { reason: 'notCollected', collected: COLLECTED, own: OWN_APART },
    ],
    [
      'a range the collection covers in part names both',
      { coverage: other(true) },
      { reason: 'partlyCollected', collected: COLLECTED, own: OWN_OVERLAPPING },
    ],
    [
      'a filter does not explain a range the collection leaves out',
      { coverage: other(false), query: 'nope' },
      { reason: 'notCollected', collected: COLLECTED, own: OWN_APART },
    ],
    [
      'a filter explains a range the collection covers in part',
      { coverage: other(true), query: 'nope' },
      { reason: 'noMatches', query: 'nope' },
    ],
    [
      'hidden sources come before a range the collection leaves out',
      { coverage: other(false), anyShown: false },
      { reason: 'allHidden' },
    ],
    [
      'a failure comes before a range the collection leaves out',
      { coverage: other(false), status: failed('lostBackend', 'unreachable') },
      { reason: 'failed', cause: 'unfinished' },
    ],
  ])('%s', (_name, input, expected) => {
    expect(timelineEmptyState({ ...base, ...input })).toEqual(expected)
  })
})

describe('timelineEmptyView', () => {
  it('says why the list is empty and offers what helps', () => {
    expect(timelineEmptyView({ reason: 'noMatches', query: 'nope' })).toEqual({
      icon: 'search_off',
      title: 'No matches for “nope”',
      message: 'Try a different term, or filter by repo, branch, ticket key or author.',
      action: { kind: 'clear-search' },
    })
    expect(
      timelineEmptyView({ reason: 'notCollected', collected: COLLECTED, own: OWN_APART })
    ).toEqual({
      icon: 'date_range',
      title: 'This range is not collected',
      message: 'The activity on screen covers 1 May – 7 May 2026, not 10 May – 17 May 2026.',
      action: { kind: 'collect-own-range', label: 'Collect 10 May – 17 May 2026' },
    })
    expect(timelineEmptyView({ reason: 'nothingRecorded' })).toEqual({
      icon: 'event_busy',
      title: 'Nothing recorded in this range',
      message: 'Pick a wider date range and collect again.',
      action: null,
    })
  })

  it('tells the reasons it shares with the calendar views in their words, with its icons', () => {
    expect(timelineEmptyView({ reason: 'failed', cause: 'unfinished' })).toMatchObject({
      icon: 'cloud_off',
      title: 'No activity loaded',
    })
    expect(timelineEmptyView({ reason: 'allHidden' }).icon).toBe('filter_alt_off')
    expect(timelineEmptyView({ reason: 'noSources' }).icon).toBe('power_off')
    expect(
      timelineEmptyView({ reason: 'partlyCollected', collected: COLLECTED, own: OWN_OVERLAPPING })
    ).toMatchObject({
      icon: 'date_range',
      action: { kind: 'collect-own-range', label: 'Collect 5 May – 17 May 2026' },
    })
  })
})
