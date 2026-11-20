import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { MockBackendTransport } from '../../testing/activity-fixtures'
import { BackendFailure } from './backend-failure'
import { openEventStream, RUNTIME_EVENTS_URL, RUNTIME_SILENCE_MS } from './http-runtime-stream'
import type { RuntimeStreamHandlers } from './runtime-stream'

/** One request the stream made, with the body the test writes to. */
interface Served {
  readonly url: string
  readonly init: RequestInit
  write(text: string): void
  end(): void
  aborted(): boolean
}

/** A `fetch` the test drives: it answers each request with `answer`, a body it writes itself. */
function fakeFetch(
  answer: (body: ReadableStream<Uint8Array>) => Response = body =>
    new Response(body, { headers: { 'Content-Type': 'text/event-stream' } })
): Served[] {
  const served: Served[] = []
  vi.stubGlobal('fetch', (url: string, init: RequestInit): Promise<Response> => {
    let controller: ReadableStreamDefaultController<Uint8Array> | null = null
    const body = new ReadableStream<Uint8Array>({
      start: (c): void => void (controller = c),
    })
    const encoder = new TextEncoder()
    let aborted = false
    init.signal?.addEventListener('abort', () => {
      aborted = true
      controller?.error(init.signal?.reason)
    })
    served.push({
      url,
      init,
      // Nothing reaches a request that was aborted, as on a socket that is gone.
      write: text => {
        if (!aborted) controller?.enqueue(encoder.encode(text))
      },
      end: () => {
        if (!aborted) controller?.close()
      },
      aborted: () => aborted,
    })
    return Promise.resolve(answer(body))
  })
  return served
}

/** Handlers that record what the stream delivered, in order, and count its progress. */
function recording(): { handlers: RuntimeStreamHandlers; seen: unknown[]; progress: () => number } {
  const seen: unknown[] = []
  let progress = 0
  return {
    seen,
    progress: () => progress,
    handlers: {
      state: pair => seen.push(['state', pair.activity.revision]),
      activity: update => seen.push(['activity', update.revision]),
      providers: update => seen.push(['providers', update.revision]),
      lost: failure => seen.push(['lost', failure]),
      progress: () => void progress++,
    },
  }
}

const event = (name: string, payload: unknown): string =>
  `event: ${name}\ndata: ${JSON.stringify(payload)}\n\n`

const { activity, providers } = new MockBackendTransport().runtimeState()
const pair = { activity, providers }

/** Lets the stream read what was written. */
const flush = (): Promise<unknown> => vi.advanceTimersByTimeAsync(0)

describe('the HTTP event stream', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => {
    vi.useRealTimers()
    vi.unstubAllGlobals()
  })

  it('follows GET /api/events and delivers each event as it comes, however it is split', async () => {
    const served = fakeFetch()
    const { handlers, seen } = recording()

    openEventStream(handlers)
    await flush()
    const text = event('state', pair) + event('providers', { ...providers, revision: 5 })
    served[0]?.write(text.slice(0, 7))
    served[0]?.write(text.slice(7, 80))
    served[0]?.write(text.slice(80))
    await flush()

    expect(served[0]?.url).toBe(RUNTIME_EVENTS_URL)
    expect(new Headers(served[0]?.init.headers).get('Accept')).toBe('text/event-stream')
    expect(seen).toEqual([
      ['state', activity.revision],
      ['providers', 5],
    ])
  })

  it('keeps a large event that takes longer than the silence rule, as long as bytes come', async () => {
    const served = fakeFetch()
    const { handlers, seen, progress } = recording()
    openEventStream(handlers)
    await flush()
    const text = event('state', pair)
    const step = Math.ceil(text.length / 20)

    // One event over twice the silence time, a little of it every few seconds.
    for (let at = 0; at < text.length; at += step) {
      await vi.advanceTimersByTimeAsync(RUNTIME_SILENCE_MS / 10)
      served[0]?.write(text.slice(at, at + step))
      await flush()
    }

    expect(seen).toEqual([['state', activity.revision]])
    expect(progress()).toBe(20)
    expect(served[0]?.aborted()).toBe(false)
  })

  it('counts a connection that sends nothing for the silence time as lost, and aborts it', async () => {
    const served = fakeFetch()
    const { handlers, seen } = recording()
    openEventStream(handlers)
    await flush()
    served[0]?.write(event('state', pair))
    await flush()

    await vi.advanceTimersByTimeAsync(RUNTIME_SILENCE_MS - 1)
    served[0]?.write(': half a line of nothing')
    await flush()
    await vi.advanceTimersByTimeAsync(RUNTIME_SILENCE_MS)

    expect(seen).toEqual([
      ['state', activity.revision],
      ['lost', expect.objectContaining({ kind: 'unreachable' })],
    ])
    expect(served[0]?.aborted()).toBe(true)
  })

  it('ends when the server ends the stream', async () => {
    const served = fakeFetch()
    const { handlers, seen } = recording()
    openEventStream(handlers)
    await flush()

    served[0]?.write(event('state', pair))
    served[0]?.end()
    await flush()

    expect(seen).toEqual([
      ['state', activity.revision],
      ['lost', expect.objectContaining({ kind: 'unreachable' })],
    ])
  })

  it('ends with the kind the backend refused the stream with', async () => {
    fakeFetch(
      () =>
        new Response(JSON.stringify({ kind: 'notFound', error: '404 Not Found' }), {
          status: 404,
          headers: { 'Content-Type': 'application/json' },
        })
    )
    const { handlers, seen } = recording()

    openEventStream(handlers)
    await flush()

    expect(seen).toEqual([['lost', expect.objectContaining({ kind: 'notFound' })]])
  })

  it('ends at an answer that is no event stream, such as a proxy’s page', async () => {
    fakeFetch(() => new Response('<html>Bad Gateway</html>', { status: 502 }))
    const { handlers, seen } = recording()

    openEventStream(handlers)
    await flush()

    expect(seen).toEqual([['lost', expect.objectContaining({ kind: 'internal' })]])
    expect(String(seen[0])).not.toContain('<html>')
  })

  it('ends as unreachable when no answer comes at all', async () => {
    const refused = new TypeError('Failed to fetch')
    vi.stubGlobal('fetch', () => Promise.reject(refused))
    const { handlers, seen } = recording()

    openEventStream(handlers)
    await flush()

    expect(seen).toEqual([
      ['lost', expect.objectContaining({ kind: 'unreachable', cause: refused })],
    ])
  })

  it.each([
    ['an empty pair', 'state', {}],
    ['a pair of two hosts', 'state', { activity, providers: { ...providers, epoch: 'other' } }],
    ['an activity of an unknown kind', 'activity', { ...activity, state: { kind: 'later' } }],
    ['providers without a list', 'providers', { ...providers, providers: { currentOs: 'linux' } }],
    ['a revision that is no integer', 'activity', { ...activity, revision: 1.5 }],
    ['a ping that is not one', 'ping', { host: 'a' }],
  ] as const)('ends at %s, delivering nothing of it', async (_name, name, payload) => {
    const served = fakeFetch()
    const { handlers, seen } = recording()
    openEventStream(handlers)
    await flush()

    served[0]?.write(event(name, payload))
    await flush()

    expect(seen).toEqual([['lost', expect.objectContaining({ kind: 'internal' })]])
    expect(seen[0]).toEqual(['lost', expect.any(BackendFailure)])
    expect(served[0]?.aborted()).toBe(true)
  })

  it('ends at data that is not JSON', async () => {
    const served = fakeFetch()
    const { handlers, seen } = recording()
    openEventStream(handlers)
    await flush()

    served[0]?.write('event: state\ndata: not json\n\n')
    await flush()

    expect(seen).toEqual([['lost', expect.objectContaining({ kind: 'internal' })]])
  })

  it('aborts its request when closed and delivers nothing after', async () => {
    const served = fakeFetch()
    const { handlers, seen } = recording()
    const stream = openEventStream(handlers)
    await flush()

    stream.close()
    served[0]?.write(event('state', pair))
    await vi.advanceTimersByTimeAsync(RUNTIME_SILENCE_MS * 2)

    expect(served[0]?.aborted()).toBe(true)
    expect(seen).toEqual([])
  })

  it('takes the event stream type in any case and with parameters, and nothing longer', async () => {
    const answered =
      (type: string) =>
      (body: ReadableStream<Uint8Array>): Response =>
        new Response(body, { headers: { 'Content-Type': type } })
    const served = fakeFetch(answered('Text/Event-Stream; charset=utf-8'))
    const kept = recording()
    openEventStream(kept.handlers)
    await flush()
    served[0]?.write(event('state', pair))
    await flush()
    expect(kept.seen).toEqual([['state', activity.revision]])

    fakeFetch(answered('text/event-stream-extra'))
    const refused = recording()
    openEventStream(refused.handlers)
    await flush()
    expect(refused.seen).toEqual([['lost', expect.objectContaining({ kind: 'internal' })]])
  })

  it('ignores an event it does not know, whatever its data', async () => {
    const served = fakeFetch()
    const { handlers, seen } = recording()
    openEventStream(handlers)
    await flush()

    served[0]?.write('event: later\ndata: plain text\n\n' + event('state', pair))
    await flush()

    expect(seen).toEqual([['state', activity.revision]])
    expect(served[0]?.aborted()).toBe(false)
  })

  it('keeps why a refusal could not be read as its cause', async () => {
    const broken = new Error('connection reset while reading the body')
    fakeFetch(
      () =>
        new Response(new ReadableStream({ start: (c): void => c.error(broken) }), {
          status: 500,
          headers: { 'Content-Type': 'application/json' },
        })
    )
    const { handlers, seen } = recording()

    openEventStream(handlers)
    await flush()

    expect(seen).toEqual([['lost', expect.objectContaining({ kind: 'internal', cause: broken })]])
  })
})
