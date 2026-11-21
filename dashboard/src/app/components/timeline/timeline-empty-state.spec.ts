import { describe, expect, it } from 'vitest'
import { BackendFailure } from '../../services/backend/backend-failure'
import { failureOf } from '../../services/failure'
import type { CollectionStatus, FailureOrigin } from '../../services/types'
import type { RangeCoverage } from '../../services/activity-selection.service'
import { timelineEmptyState, type TimelineEmptyInput } from './timeline-empty-state'

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

/** The collection on screen covers 1–7 May; this tab's range is 10–17 May, or 5–17 May. */
const other = (overlaps: boolean): RangeCoverage => ({
  kind: 'other',
  collected: { from: '2026-05-01', to: '2026-05-07' },
  own: { from: overlaps ? '2026-05-05' : '2026-05-10', to: '2026-05-17' },
  overlaps,
})

describe('timelineEmptyState', () => {
  it.each<
    [string, Partial<TimelineEmptyInput>, { title: string; message: RegExp; action: string | null }]
  >([
    [
      'a collect refused as invalid asks for another range',
      { status: failed('collect', 'invalidRequest'), query: 'x' },
      { title: 'No activity loaded', message: /Choose another one/, action: null },
    ],
    [
      'a refused read offers no retry',
      { status: failed('refusedRead', 'invalidRequest') },
      { title: 'No activity loaded', message: /^(?!.*Retry).*check source status/, action: null },
    ],
    [
      'a lost backend points at Retry',
      { status: failed('lostBackend', 'unreachable') },
      { title: 'No activity loaded', message: /Retry above/, action: null },
    ],
    [
      'a filter that matches nothing offers to clear it',
      { query: 'nope', hasSources: false },
      { title: 'No matches for “nope”', message: /different term/, action: 'clear-search' },
    ],
    [
      'no sources sends to Settings',
      { hasSources: false, anyShown: false },
      { title: 'No sources are ready', message: /Open Settings/, action: null },
    ],
    [
      'every source hidden sends to the sidebar',
      { anyShown: false },
      { title: 'All sources are hidden', message: /sidebar/, action: null },
    ],
    [
      'an empty range asks for a wider one',
      {},
      { title: 'Nothing recorded in this range', message: /wider date range/, action: null },
    ],
    [
      'without a range to compare, an empty range asks for a wider one',
      { coverage: { kind: 'unknown' } },
      { title: 'Nothing recorded in this range', message: /wider date range/, action: null },
    ],
    [
      'a range the collection leaves out names both and offers to collect it',
      { coverage: other(false) },
      {
        title: 'This range is not collected',
        message: /^The activity on screen covers 1 May – 7 May 2026, not 10 May – 17 May 2026\.$/,
        action: 'collect-own-range',
      },
    ],
    [
      'a range the collection covers in part names both and offers to collect it',
      { coverage: other(true) },
      {
        title: 'Nothing recorded in the collected days',
        message: /covers 1 May – 7 May 2026; the rest of 5 May – 17 May 2026 is not collected/,
        action: 'collect-own-range',
      },
    ],
    [
      'a filter does not explain a range the collection leaves out',
      { coverage: other(false), query: 'nope' },
      { title: 'This range is not collected', message: /not 10 May/, action: 'collect-own-range' },
    ],
    [
      'a filter explains a range the collection covers in part',
      { coverage: other(true), query: 'nope' },
      { title: 'No matches for “nope”', message: /different term/, action: 'clear-search' },
    ],
    [
      'hidden sources come before a range the collection leaves out',
      { coverage: other(false), anyShown: false },
      { title: 'All sources are hidden', message: /sidebar/, action: null },
    ],
    [
      'a failure comes before a range the collection leaves out',
      { coverage: other(false), status: failed('lostBackend', 'unreachable') },
      { title: 'No activity loaded', message: /Retry above/, action: null },
    ],
  ])('%s', (_name, input, expected) => {
    const state = timelineEmptyState({ ...base, ...input })

    expect(state.title).toBe(expected.title)
    expect(state.message).toMatch(expected.message)
    expect(state.action?.kind ?? null).toBe(expected.action)
  })

  it('labels the action with this tab’s range', () => {
    const state = timelineEmptyState({ ...base, coverage: other(false) })

    expect(state.action).toEqual({
      kind: 'collect-own-range',
      label: 'Collect 10 May – 17 May 2026',
    })
  })
})
