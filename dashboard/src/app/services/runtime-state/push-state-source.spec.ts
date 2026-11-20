import { TestBed } from '@angular/core/testing'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { RuntimeState } from '../../generated/activity'
import { MockBackendTransport } from '../../testing/activity-fixtures'
import { BackendFailure } from '../backend/backend-failure'
import { BackendTransport } from '../backend/backend-transport'
import type { RuntimeStream, RuntimeStreamHandlers } from '../backend/runtime-stream'
import { FIRST_PAIR_TIMEOUT_MS, PushStateSource, RECONNECT_DELAYS_MS } from './push-state-source'
import type { RuntimeStateSink, SourceStatus } from './runtime-state-source'

/** A stream the test drives. */
interface FakeStream {
  readonly handlers: RuntimeStreamHandlers
  closed: boolean
}

/** The transport seam: every stream the source opens, newest last. */
class FakeStreams {
  readonly opened: FakeStream[] = []

  openRuntimeStream(handlers: RuntimeStreamHandlers): RuntimeStream {
    const stream: FakeStream = { handlers, closed: false }
    this.opened.push(stream)
    return { close: () => (stream.closed = true) }
  }

  latest(): FakeStream {
    const stream = this.opened.at(-1)
    if (!stream) throw new Error('no stream opened')
    return stream
  }
}

/** What the sink got, in order. */
type Seen =
  | readonly ['state', number, boolean]
  | readonly ['activity', number]
  | readonly ['providers', number]
  | readonly ['status', SourceStatus['kind']]

function pairAt(revision: number): RuntimeState {
  const { activity, providers } = new MockBackendTransport().runtimeState()
  return { activity: { ...activity, revision }, providers: { ...providers, revision } }
}

const unreachable = new BackendFailure('unreachable', 'connection refused')
const delays = [500, 1000]

function setup(
  onPair: (sink: PushStateSource) => void = () => undefined,
  schedule: readonly number[] | 'production' = delays
): {
  streams: FakeStreams
  source: PushStateSource
  seen: Seen[]
  statuses: SourceStatus[]
} {
  const streams = new FakeStreams()
  TestBed.configureTestingModule({
    providers: [
      { provide: BackendTransport, useValue: streams },
      ...(schedule === 'production' ? [] : [{ provide: RECONNECT_DELAYS_MS, useValue: schedule }]),
    ],
  })
  const source = TestBed.inject(PushStateSource)
  const seen: Seen[] = []
  const statuses: SourceStatus[] = []
  const sink: RuntimeStateSink = {
    state: (pair, authoritative) => {
      seen.push(['state', pair.activity.revision, authoritative])
      onPair(source)
    },
    activity: update => seen.push(['activity', update.revision]),
    providers: update => seen.push(['providers', update.revision]),
    connection: status => {
      seen.push(['status', status.kind])
      statuses.push(status)
    },
  }
  source.connect(sink)
  return { streams, source, seen, statuses }
}

describe('PushStateSource', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('takes each stream’s first pair as authoritative, and what follows as it comes', () => {
    const { streams, seen } = setup()
    const stream = streams.latest()
    const pair = pairAt(3)

    stream.handlers.state(pair)
    stream.handlers.activity({ ...pair.activity, revision: 4 })
    stream.handlers.providers({ ...pair.providers, revision: 5 })
    stream.handlers.state(pairAt(6))

    expect(seen).toEqual([
      ['status', 'connecting'],
      ['state', 3, true],
      ['status', 'open'],
      ['activity', 4],
      ['providers', 5],
      ['state', 6, false],
    ])
  })

  it('connects again after each delay, then gives up with the last failure', async () => {
    const { streams, statuses } = setup()

    streams.latest().handlers.lost(unreachable)
    expect(statuses.at(-1)).toEqual({
      kind: 'reconnecting',
      failure: expect.objectContaining({ message: 'connection refused' }),
    })
    await vi.advanceTimersByTimeAsync(delays[0]! - 1)
    expect(streams.opened).toHaveLength(1)
    await vi.advanceTimersByTimeAsync(1)
    expect(streams.opened).toHaveLength(2)

    streams.latest().handlers.lost(unreachable)
    await vi.advanceTimersByTimeAsync(delays[1]!)
    expect(streams.opened).toHaveLength(3)
    streams.latest().handlers.lost(unreachable)
    await vi.runAllTimersAsync()

    expect(streams.opened).toHaveLength(3)
    expect(statuses.at(-1)).toEqual({
      kind: 'failed',
      cause: {
        kind: 'unreachable',
        failure: { kind: 'backend', message: 'connection refused', cause: unreachable },
      },
    })
  })

  it('starts the delays over once a stream brought its pair', async () => {
    const { streams, seen } = setup()
    streams.latest().handlers.lost(unreachable)
    await vi.advanceTimersByTimeAsync(delays[0]!)
    streams.latest().handlers.state(pairAt(1))

    // That stream is lost later: the first delay again, and its pair is authoritative.
    streams.latest().handlers.lost(unreachable)
    await vi.advanceTimersByTimeAsync(delays[0]!)
    streams.latest().handlers.state(pairAt(2))

    expect(streams.opened).toHaveLength(3)
    expect(seen.filter(([kind]) => kind === 'state')).toEqual([
      ['state', 1, true],
      ['state', 2, true],
    ])
  })

  it('counts a stream whose first message is a part as lost, delivering none of it', async () => {
    const { streams, seen, statuses } = setup()
    const stream = streams.latest()

    stream.handlers.activity(pairAt(1).activity)
    stream.handlers.providers(pairAt(2).providers)

    expect(stream.closed).toBe(true)
    expect(seen.filter(([kind]) => kind !== 'status')).toEqual([])
    expect(statuses.at(-1)).toMatchObject({
      kind: 'reconnecting',
      failure: { cause: { kind: 'internal' } },
    })
    await vi.advanceTimersByTimeAsync(delays[0]!)
    streams.latest().handlers.state(pairAt(3))
    expect(seen.at(-2)).toEqual(['state', 3, true])
  })

  it('gives up at once on a stream the backend refuses for good', () => {
    const { streams, statuses } = setup()
    const refused = new BackendFailure('notFound', 'No such command')

    streams.latest().handlers.lost(refused)

    expect(statuses.at(-1)).toMatchObject({ kind: 'failed', cause: { kind: 'refused' } })
    expect(streams.opened).toHaveLength(1)
  })

  it('counts a stream without its pair in time as lost', async () => {
    const { streams, statuses } = setup()

    await vi.advanceTimersByTimeAsync(FIRST_PAIR_TIMEOUT_MS)

    expect(streams.opened[0]?.closed).toBe(true)
    expect(statuses.at(-1)).toMatchObject({
      kind: 'reconnecting',
      failure: { cause: { kind: 'unreachable' } },
    })
    await vi.advanceTimersByTimeAsync(delays[0]!)
    expect(streams.opened).toHaveLength(2)
  })

  it('gives a pair still arriving its time again with each chunk, and none once bytes stop', async () => {
    const { streams, statuses } = setup()
    const stream = streams.latest()

    for (let chunk = 0; chunk < 5; chunk++) {
      await vi.advanceTimersByTimeAsync(FIRST_PAIR_TIMEOUT_MS - 1)
      stream.handlers.progress()
    }
    expect(stream.closed).toBe(false)
    stream.handlers.state(pairAt(1))
    expect(statuses.at(-1)).toEqual({ kind: 'open' })

    // Progress after the pair changes nothing; the transport's own silence rule owns that.
    stream.handlers.progress()
    expect(statuses.at(-1)).toEqual({ kind: 'open' })
  })

  it('retries with a new stream whose pair is authoritative, the old one delivering nothing', () => {
    const { streams, source, seen } = setup()
    const old = streams.latest()
    old.handlers.state(pairAt(1))

    source.retry()
    old.handlers.activity(pairAt(2).activity)
    old.handlers.lost(unreachable)
    streams.latest().handlers.state(pairAt(3))

    expect(old.closed).toBe(true)
    expect(streams.opened).toHaveLength(2)
    expect(seen).toEqual([
      ['status', 'connecting'],
      ['state', 1, true],
      ['status', 'open'],
      ['status', 'connecting'],
      ['state', 3, true],
      ['status', 'open'],
    ])
  })

  it('leaves the reporting to the new stream when taking a pair asked for one', () => {
    let asked = false
    const { streams, seen } = setup(source => {
      if (asked) return
      asked = true
      source.retry()
    })

    streams.latest().handlers.state(pairAt(1))

    expect(seen).toEqual([
      ['status', 'connecting'],
      ['state', 1, true],
      ['status', 'connecting'],
    ])
    expect(streams.opened).toHaveLength(2)
  })

  it('connects again when a command is answered after it gave up, and only then', async () => {
    const { streams, source } = setup()
    streams.latest().handlers.state(pairAt(1))
    source.follow()
    expect(streams.opened).toHaveLength(1)

    streams.latest().handlers.lost(new BackendFailure('notFound', 'gone'))
    source.follow()

    expect(streams.opened).toHaveLength(2)
  })

  it('settles once the first pair is in, or once it gave up', async () => {
    const { streams, source } = setup()
    let settled = false
    void source.settled().then(() => (settled = true))

    streams.latest().handlers.lost(unreachable)
    await vi.advanceTimersByTimeAsync(delays[0]!)
    expect(settled).toBe(false)
    streams.latest().handlers.state(pairAt(1))
    await vi.advanceTimersByTimeAsync(0)
    expect(settled).toBe(true)

    source.retry()
    let again = false
    void source.settled().then(() => (again = true))
    streams.latest().handlers.lost(new BackendFailure('notFound', 'gone'))
    await vi.advanceTimersByTimeAsync(0)
    expect(again).toBe(true)
  })

  it('closes its stream and its timers when torn down, and delivers nothing after', async () => {
    const { streams, seen } = setup()
    const stream = streams.latest()
    stream.handlers.lost(unreachable)
    await vi.advanceTimersByTimeAsync(delays[0]!)
    const reconnected = streams.latest()
    const before = seen.length

    TestBed.resetTestingModule()
    reconnected.handlers.state(pairAt(1))
    await vi.runAllTimersAsync()

    expect(reconnected.closed).toBe(true)
    expect(seen).toHaveLength(before)
    expect(streams.opened).toHaveLength(2)
  })

  it('connects again after 0.5, 1, 2, 4 and 8 s, then gives up', async () => {
    const { streams, statuses } = setup(undefined, 'production')

    for (const delay of [500, 1000, 2000, 4000, 8000]) {
      const lost = streams.opened.length
      streams.latest().handlers.lost(unreachable)
      await vi.advanceTimersByTimeAsync(delay - 1)
      expect(streams.opened).toHaveLength(lost)
      await vi.advanceTimersByTimeAsync(1)
      expect(streams.opened).toHaveLength(lost + 1)
    }
    streams.latest().handlers.lost(unreachable)

    expect(statuses.at(-1)).toMatchObject({ kind: 'failed', cause: { kind: 'unreachable' } })
  })

  it('gives up at once on an operation this runtime does not have', () => {
    const { streams, statuses } = setup()

    streams.latest().handlers.lost(new BackendFailure('unsupported', 'not here'))

    expect(statuses.at(-1)).toMatchObject({ kind: 'failed', cause: { kind: 'refused' } })
    expect(streams.opened).toHaveLength(1)
  })

  it('still counts a stream lost when bytes stop after one partial chunk', async () => {
    const { streams, statuses } = setup()
    const stream = streams.latest()
    await vi.advanceTimersByTimeAsync(FIRST_PAIR_TIMEOUT_MS / 2)
    stream.handlers.progress()

    await vi.advanceTimersByTimeAsync(FIRST_PAIR_TIMEOUT_MS - 1)
    expect(stream.closed).toBe(false)
    await vi.advanceTimersByTimeAsync(1)

    expect(stream.closed).toBe(true)
    expect(statuses.at(-1)).toMatchObject({ kind: 'reconnecting' })
  })

  it('drops the pair of a stream that a reconnect retired', async () => {
    const { streams, seen } = setup()
    const retired = streams.latest()
    retired.handlers.lost(unreachable)
    await vi.advanceTimersByTimeAsync(delays[0]!)

    retired.handlers.state(pairAt(1))
    streams.latest().handlers.state(pairAt(2))

    expect(seen.filter(([kind]) => kind === 'state')).toEqual([['state', 2, true]])
  })

  it('settles when torn down before any pair', async () => {
    const { source } = setup()
    let settled = false
    void source.settled().then(() => (settled = true))

    TestBed.resetTestingModule()
    await vi.advanceTimersByTimeAsync(0)

    expect(settled).toBe(true)
  })
})
