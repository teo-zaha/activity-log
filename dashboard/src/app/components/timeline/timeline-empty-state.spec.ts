import { describe, expect, it } from 'vitest'
import { BackendFailure } from '../../services/backend/backend-failure'
import { failureOf } from '../../services/failure'
import type { CollectionStatus, FailureOrigin } from '../../services/types'
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
}

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
  ])('%s', (_name, input, expected) => {
    const state = timelineEmptyState({ ...base, ...input })

    expect(state.title).toBe(expected.title)
    expect(state.message).toMatch(expected.message)
    expect(state.action?.kind ?? null).toBe(expected.action)
  })
})
