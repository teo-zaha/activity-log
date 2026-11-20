import { TestBed } from '@angular/core/testing'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { RuntimeState } from '../../generated/activity'
import { activityLog, providersResponse } from '../../testing/activity-fixtures'
import { ActivityBackendClient } from '../backend/backend-client'
import { BackendFailure } from '../backend/backend-failure'
import {
  ACTIVITY_POLL_ATTEMPTS,
  ACTIVITY_POLL_DELAY_MS,
  PollingStateSource,
} from './polling-state-source'
import type { RuntimeStateSink, SourceStatus } from './runtime-state-source'

const pairOf = (epoch: string): RuntimeState => ({
  activity: { epoch, authority: '1', revision: 1, state: { kind: 'ready', log: activityLog } },
  providers: { epoch, authority: '1', revision: 2, providers: providersResponse },
})

/** A backend whose reads answer when the test says, whatever their signal: one that cannot abort. */
class HeldReads {
  readonly answers: ((pair: RuntimeState) => void)[] = []
  getRuntimeState(): Promise<RuntimeState> {
    return new Promise(resolve => this.answers.push(resolve))
  }
}

function connect(): {
  backend: HeldReads
  source: PollingStateSource
  pairs: [RuntimeState, boolean][]
  statuses: SourceStatus['kind'][]
} {
  const backend = new HeldReads()
  TestBed.configureTestingModule({
    providers: [
      { provide: ActivityBackendClient, useValue: backend },
      { provide: ACTIVITY_POLL_DELAY_MS, useValue: 1000 },
    ],
  })
  const source = TestBed.inject(PollingStateSource)
  const pairs: [RuntimeState, boolean][] = []
  const statuses: SourceStatus['kind'][] = []
  const sink: RuntimeStateSink = {
    state: (pair, authoritative) => pairs.push([pair, authoritative]),
    activity: () => undefined,
    providers: () => undefined,
    connection: status => statuses.push(status.kind),
  }
  source.connect(sink)
  return { backend, source, pairs, statuses }
}

describe('PollingStateSource', () => {
  beforeEach(() => {
    vi.useFakeTimers()
  })

  afterEach(() => {
    vi.useRealTimers()
  })

  it('drops a read of the attempt Retry replaced, even one its transport cannot abort', async () => {
    const { backend, source, pairs, statuses } = connect()
    await vi.advanceTimersByTimeAsync(0)
    expect(backend.answers).toHaveLength(1)

    source.retry()
    backend.answers[0]?.(pairOf('old-host'))
    await vi.advanceTimersByTimeAsync(0)
    backend.answers[1]?.(pairOf('new-host'))
    await vi.advanceTimersByTimeAsync(0)

    expect(pairs).toEqual([[pairOf('new-host'), true]])
    expect(statuses).not.toContain('reconnecting')
  })

  it('reports no open link for an attempt the pair it delivered retired', async () => {
    const backend = new HeldReads()
    TestBed.configureTestingModule({
      providers: [{ provide: ActivityBackendClient, useValue: backend }],
    })
    const source = TestBed.inject(PollingStateSource)
    const statuses: SourceStatus['kind'][] = []
    // The coordinator asks for a handshake while it takes the pair.
    source.connect({
      state: () => source.retry(),
      activity: () => undefined,
      providers: () => undefined,
      connection: status => statuses.push(status.kind),
    })
    await vi.advanceTimersByTimeAsync(0)

    backend.answers[0]?.(pairOf('retired'))
    await vi.advanceTimersByTimeAsync(0)

    expect(statuses.at(-1)).toBe('connecting')
    expect(backend.answers).toHaveLength(2)
  })

  it('delivers nothing and reads no more once torn down during a read', async () => {
    const { backend, pairs, statuses } = connect()
    await vi.advanceTimersByTimeAsync(0)
    const reported = statuses.length

    TestBed.resetTestingModule()
    backend.answers[0]?.(pairOf('host'))
    await vi.runAllTimersAsync()

    expect(pairs).toEqual([])
    expect(statuses).toHaveLength(reported)
    expect(backend.answers).toHaveLength(1)
  })

  it.each([
    ['ends at once on a read the backend refuses for good', 'notFound', 1],
    ['tries again after a read that may work next time', 'unreachable', 3],
  ] as const)('%s', async (_name, kind, reads) => {
    const refused = new BackendFailure(kind, 'the read failed')
    const getRuntimeState = vi.fn(() => Promise.reject(refused))
    TestBed.configureTestingModule({
      providers: [
        { provide: ActivityBackendClient, useValue: { getRuntimeState } },
        { provide: ACTIVITY_POLL_DELAY_MS, useValue: 1 },
        { provide: ACTIVITY_POLL_ATTEMPTS, useValue: 3 },
      ],
    })
    const statuses: SourceStatus[] = []
    TestBed.inject(PollingStateSource).connect({
      state: () => undefined,
      activity: () => undefined,
      providers: () => undefined,
      connection: status => statuses.push(status),
    })

    await vi.runAllTimersAsync()

    expect(getRuntimeState).toHaveBeenCalledTimes(reads)
    const failed = statuses.at(-1)
    expect(failed?.kind).toBe('failed')
    // The read's own failure, never wrapped in another.
    expect(failed).toMatchObject({
      cause: {
        kind: kind === 'notFound' ? 'refused' : 'unreachable',
        failure: { kind: 'backend', cause: refused },
      },
    })
  })
})
