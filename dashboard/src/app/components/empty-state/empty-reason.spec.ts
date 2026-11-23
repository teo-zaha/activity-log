import { describe, expect, it } from 'vitest'
import { BackendFailure } from '../../services/backend/backend-failure'
import { failureOf } from '../../services/failure'
import type { CollectionStatus, FailureOrigin } from '../../services/types'
import { failedReason, partlyCollected, sharedEmptyText, sourcesReason } from './empty-reason'

const failed = (
  origin: FailureOrigin,
  kind: 'invalidRequest' | 'unreachable'
): CollectionStatus => ({
  kind: 'failed',
  operation: 'collect',
  origin,
  failure: failureOf(new BackendFailure(kind, 'refused')),
})

const COLLECTED = { from: '2026-05-01', to: '2026-05-07' } as const
const OWN = { from: '2026-05-05', to: '2026-05-17' } as const

describe('the reasons the Timeline and the calendar views share', () => {
  it('tells a failed collection by what would help', () => {
    expect(failedReason({ kind: 'idle' })).toBeNull()
    expect(failedReason(failed('collect', 'invalidRequest'))).toEqual({
      reason: 'failed',
      cause: 'refusedRange',
    })
    expect(failedReason(failed('refusedRead', 'invalidRequest'))).toEqual({
      reason: 'failed',
      cause: 'refusedRead',
    })
    expect(failedReason(failed('collection', 'unreachable'))).toEqual({
      reason: 'failed',
      cause: 'unfinished',
    })
  })

  it('tells no ready source from every source hidden', () => {
    expect(sourcesReason({ status: { kind: 'idle' }, hasSources: true, anyShown: true })).toBeNull()
    expect(sourcesReason({ status: { kind: 'idle' }, hasSources: false, anyShown: false })).toEqual(
      { reason: 'noSources' }
    )
    expect(sourcesReason({ status: { kind: 'idle' }, hasSources: true, anyShown: false })).toEqual({
      reason: 'allHidden',
    })
  })

  it('reads only a collection that holds part of this tab’s range as partly collected', () => {
    expect(partlyCollected({ kind: 'covered' })).toBeNull()
    expect(partlyCollected({ kind: 'unknown' })).toBeNull()
    expect(
      partlyCollected({ kind: 'other', collected: COLLECTED, own: OWN, overlaps: false })
    ).toBeNull()
    expect(
      partlyCollected({ kind: 'other', collected: COLLECTED, own: OWN, overlaps: true })
    ).toEqual({ reason: 'partlyCollected', collected: COLLECTED, own: OWN })
  })

  it('words each reason once', () => {
    expect(sharedEmptyText({ reason: 'failed', cause: 'refusedRange' })).toEqual({
      title: 'No activity loaded',
      message: 'The backend refused to collect this range. Choose another one.',
      action: null,
    })
    expect(sharedEmptyText({ reason: 'failed', cause: 'refusedRead' }).message).toBe(
      'The backend refused to show its activity; check source status in Settings.'
    )
    expect(sharedEmptyText({ reason: 'failed', cause: 'unfinished' }).message).toBe(
      'The last collection did not finish. Retry above, or check source status in Settings.'
    )
    expect(sharedEmptyText({ reason: 'noSources' })).toEqual({
      title: 'No sources are ready',
      message: 'Open Settings to see why each source is unavailable and configure it.',
      action: null,
    })
    expect(sharedEmptyText({ reason: 'allHidden' })).toEqual({
      title: 'All sources are hidden',
      message: 'Turn on at least one source in the sidebar to see its activity.',
      action: null,
    })
    expect(sharedEmptyText({ reason: 'partlyCollected', collected: COLLECTED, own: OWN })).toEqual({
      title: 'Nothing recorded in the collected days',
      message:
        'The activity on screen covers 1 May – 7 May 2026; the rest of 5 May – 17 May 2026 is not collected.',
      action: { kind: 'collect-range', range: OWN, label: 'Collect 5 May – 17 May 2026' },
    })
  })
})
