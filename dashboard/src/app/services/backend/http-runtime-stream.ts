import { RUNTIME_PING_SECONDS } from '../../generated/activity'
import { BackendFailure, responseFailureOf } from './backend-failure'
import { decodeRuntimeMessage, isRuntimePing } from './runtime-message'
import { deliver, type RuntimeStream, type RuntimeStreamHandlers } from './runtime-stream'
import { SseParser, type SseEvent } from './sse-parser'

/** The host's state as server-sent events. */
export const RUNTIME_EVENTS_URL = '/api/events'

/**
 * How long the event stream may bring no bytes before it counts as lost: two `ping`s missed, and
 * some margin. Bytes, not events: a large event on a slow link takes longer than this to come
 * whole, and the `ping` behind it cannot pass it.
 */
export const RUNTIME_SILENCE_MS = 2 * RUNTIME_PING_SECONDS * 1000 + 5000

/**
 * One connection to `GET /api/events`, read with `fetch` and parsed here (`SseParser`), so the
 * stream sees each chunk as it arrives: every chunk renews the silence rule and is reported as
 * `progress`, while an `EventSource` would only show whole events. The backend's refusal keeps
 * its kind (an API without the route is `notFound`); an answer that is no event stream, a stream
 * that ends or breaks off, a message that does not fit (`decodeRuntimeMessage`) and
 * `RUNTIME_SILENCE_MS` without a byte end it as `lost`. Closing aborts the request. The fetch
 * and the timer are hand-written: Signality 0.5 covers neither a streamed body nor a timer that
 * lives with a connection outside any injection context.
 */
export function openEventStream(handlers: RuntimeStreamHandlers): RuntimeStream {
  const request = new AbortController()
  let open = true
  let silence: ReturnType<typeof setTimeout> | null = null
  const close = (): void => {
    open = false
    if (silence !== null) clearTimeout(silence)
    request.abort()
  }
  const lose = (failure: BackendFailure): void => {
    if (!open) return
    close()
    handlers.lost(failure)
  }
  const heard = (): void => {
    if (silence !== null) clearTimeout(silence)
    silence = setTimeout(
      () =>
        lose(
          new BackendFailure(
            'unreachable',
            `The backend sent nothing for ${RUNTIME_SILENCE_MS / 1000} s`
          )
        ),
      RUNTIME_SILENCE_MS
    )
  }
  const receive = ({ event, data }: SseEvent): void => {
    // An event this dashboard does not know is ignored, whatever its data, as an `EventSource`
    // would ignore one without a listener.
    if (event !== 'ping' && event !== 'state' && event !== 'activity' && event !== 'providers') {
      return
    }
    let payload: unknown
    try {
      payload = JSON.parse(data)
    } catch (error) {
      return lose(
        new BackendFailure('internal', 'The backend sent an event that is not JSON', {
          cause: error,
        })
      )
    }
    if (event === 'ping') {
      if (!isRuntimePing(payload)) lose(unreadable(event))
      return
    }
    const message = decodeRuntimeMessage(event, payload)
    if (message === null) return lose(unreadable(event))
    deliver(handlers, message)
  }
  const follow = async (): Promise<void> => {
    const response = await fetch(RUNTIME_EVENTS_URL, {
      headers: { Accept: 'text/event-stream' },
      cache: 'no-store',
      signal: request.signal,
    }).catch((error: unknown) => {
      throw new BackendFailure('unreachable', 'The backend cannot be reached', { cause: error })
    })
    if (!response.ok) throw await refusalOf(response)
    const body = response.body
    if (body === null || !isEventStream(response.headers.get('Content-Type'))) {
      throw new BackendFailure('internal', 'The backend answered the event stream with no stream')
    }
    const reader = body.getReader()
    const decoder = new TextDecoder()
    const parser = new SseParser()
    for (;;) {
      const chunk = await reader.read().catch((error: unknown) => {
        throw new BackendFailure('unreachable', 'The connection to the backend was lost', {
          cause: error,
        })
      })
      if (!open) return
      if (chunk.done) {
        throw new BackendFailure('unreachable', 'The backend ended the event stream')
      }
      heard()
      handlers.progress()
      for (const event of parser.push(decoder.decode(chunk.value, { stream: true }))) {
        if (!open) return
        receive(event)
      }
    }
  }
  heard()
  follow().catch((error: unknown) =>
    lose(
      error instanceof BackendFailure
        ? error
        : new BackendFailure('internal', 'Following the backend failed', { cause: error })
    )
  )
  return { close }
}

function unreadable(event: string): BackendFailure {
  return new BackendFailure('internal', `The backend sent a ${event} this dashboard cannot read`)
}

/**
 * A refusal with its kind when its body is the backend's `BackendError`; otherwise `internal`,
 * the body never shown, and a body that could not be read kept as its cause.
 */
async function refusalOf(response: Response): Promise<BackendFailure> {
  let body: unknown
  try {
    body = await response.json()
  } catch (error) {
    return responseFailureOf(response.status, null, error)
  }
  return responseFailureOf(response.status, body)
}

/** Whether a `Content-Type` is `text/event-stream`: type and subtype in any case, any parameters. */
function isEventStream(contentType: string | null): boolean {
  const essence = contentType?.split(';', 1)[0]?.trim().toLowerCase()
  return essence === 'text/event-stream'
}
