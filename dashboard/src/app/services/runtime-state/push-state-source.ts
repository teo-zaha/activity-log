import { DestroyRef, InjectionToken, Service, inject } from '@angular/core'
import { BackendFailure, type BackendFailureKind } from '../backend/backend-failure'
import { BackendTransport } from '../backend/backend-transport'
import type { RuntimeStream } from '../backend/runtime-stream'
import { failureOf } from '../failure'
import type { RuntimeStateSink, RuntimeStateSource, SourceStatus } from './runtime-state-source'

/**
 * How long the source waits before each new connection after one was lost without bringing a
 * pair; once these are used up it gives up (`failed`) until `retry` or `follow`.
 */
export const RECONNECT_DELAYS_MS = new InjectionToken<readonly number[]>('RECONNECT_DELAYS_MS', {
  providedIn: 'root',
  factory: (): readonly number[] => [500, 1000, 2000, 4000, 8000],
})

/**
 * How long a stream may bring nothing before its first pair is in, before it counts as lost; a
 * pair still on its way that brings bytes (`progress`) has its time again.
 */
export const FIRST_PAIR_TIMEOUT_MS = 10_000

/** Whether connecting again could get the state, by how the backend failed the stream. */
const CONNECT_AGAIN: Readonly<Record<BackendFailureKind, boolean>> = {
  unreachable: true,
  internal: true,
  busy: true,
  noData: true,
  invalidRequest: false,
  notFound: false,
  unsupported: false,
  alreadyExists: false,
  conflict: false,
  noLocation: false,
  unprotectable: false,
  badConfig: false,
  tooLarge: false,
}

/** What the source is doing. */
type Phase =
  | { readonly kind: 'idle' }
  /** A stream is open, its first pair not in yet: `deadline` ends it if it brings nothing. */
  | {
      readonly kind: 'streaming'
      readonly stream: RuntimeStream
      readonly deadline: ReturnType<typeof setTimeout>
    }
  /** The stream's first pair is in. */
  | { readonly kind: 'paired'; readonly stream: RuntimeStream }
  /** The last stream was lost; the next one starts when the timer fires. */
  | { readonly kind: 'waiting'; readonly timer: ReturnType<typeof setTimeout> }
  | { readonly kind: 'failed' }
  | { readonly kind: 'stopped' }

/**
 * Follows the host's state as it pushes it (`docs/plans/shared-contracts.md` §5): one stream
 * (`BackendTransport.openRuntimeStream`) at a time, the first pair of each stream the
 * authoritative one. A lost stream is opened again after `RECONNECT_DELAYS_MS`, the state on
 * screen staying meanwhile (`reconnecting`); a stream the backend refuses for good fails at
 * once. A retired stream delivers nothing: every callback checks it is still the current one.
 */
@Service()
export class PushStateSource implements RuntimeStateSource {
  private readonly transport = inject(BackendTransport)
  private readonly delays = inject(RECONNECT_DELAYS_MS)

  private sink: RuntimeStateSink | null = null
  private phase: Phase = { kind: 'idle' }
  /** Streams lost in a row without bringing a pair. */
  private lost = 0
  private readonly waiters: (() => void)[] = []

  constructor() {
    inject(DestroyRef).onDestroy(() => {
      this.end()
      this.phase = { kind: 'stopped' }
      this.sink = null
      this.release()
    })
  }

  connect(sink: RuntimeStateSink): void {
    if (this.sink !== null || this.phase.kind === 'stopped')
      throw new Error('The state source cannot connect again.')
    this.sink = sink
    this.restart()
  }

  /** The backend answered a command, so it may be back: a source that gave up connects again. */
  follow(): void {
    if (this.phase.kind === 'failed') this.restart()
  }

  /** A new stream now, whatever the current one does: after `failed`, or for a missed pair. */
  retry(): void {
    if (this.sink !== null && this.phase.kind !== 'stopped') this.restart()
  }

  /** Resolves once the current stream brought its first pair, or the source gave up. */
  settled(): Promise<void> {
    if (!this.busy()) return Promise.resolve()
    return new Promise(resolve => this.waiters.push(resolve))
  }

  private busy(): boolean {
    switch (this.phase.kind) {
      case 'streaming':
      case 'waiting':
        return true
      case 'paired':
      case 'idle':
      case 'failed':
      case 'stopped':
        return false
    }
  }

  private restart(): void {
    this.lost = 0
    this.report({ kind: 'connecting' })
    this.open()
  }

  /** Opens the next stream, retiring the current one. */
  private open(): void {
    this.end()
    const stream: RuntimeStream = this.transport.openRuntimeStream({
      state: pair => {
        if (!this.current(stream)) return
        const authoritative = this.phase.kind === 'streaming'
        if (this.phase.kind === 'streaming') {
          clearTimeout(this.phase.deadline)
          this.phase = { kind: 'paired', stream }
          this.lost = 0
        }
        this.sink?.state(pair, authoritative)
        // Taking the pair may have retired this stream (a handshake asked for): the new one
        // reports for itself.
        if (!this.current(stream) || !authoritative) return
        this.report({ kind: 'open' })
        this.release()
      },
      activity: update => {
        if (this.paired(stream)) this.sink?.activity(update)
      },
      providers: update => {
        if (this.paired(stream)) this.sink?.providers(update)
      },
      lost: failure => {
        if (this.current(stream)) this.reconnectAfter(failure)
      },
      progress: () => {
        // Bytes of a pair that is still on its way: it has its time again.
        if (this.phase.kind !== 'streaming' || this.phase.stream !== stream) return
        clearTimeout(this.phase.deadline)
        this.phase = { kind: 'streaming', stream, deadline: this.deadline(stream) }
      },
    })
    this.phase = { kind: 'streaming', stream, deadline: this.deadline(stream) }
  }

  /**
   * Ends `stream` once it brought nothing for `FIRST_PAIR_TIMEOUT_MS` before its first pair. A
   * plain one-shot timer per stream, which no Signality composable covers.
   */
  private deadline(stream: RuntimeStream): ReturnType<typeof setTimeout> {
    return setTimeout(() => {
      if (this.phase.kind !== 'streaming' || this.phase.stream !== stream) return
      this.reconnectAfter(
        new BackendFailure(
          'unreachable',
          `The backend sent nothing of its state for ${FIRST_PAIR_TIMEOUT_MS / 1000} s`
        )
      )
    }, FIRST_PAIR_TIMEOUT_MS)
  }

  /**
   * Whether a part of `stream` may go to the sink: only after its first pair. A part before it
   * breaks the transport's promise, so the stream counts as lost.
   */
  private paired(stream: RuntimeStream): boolean {
    if (this.phase.kind === 'paired' && this.phase.stream === stream) return true
    if (this.phase.kind === 'streaming' && this.phase.stream === stream) {
      this.reconnectAfter(
        new BackendFailure('internal', 'The backend sent a part before the state it starts with')
      )
    }
    return false
  }

  private current(stream: RuntimeStream): boolean {
    return (
      (this.phase.kind === 'streaming' || this.phase.kind === 'paired') &&
      this.phase.stream === stream
    )
  }

  private reconnectAfter(failure: BackendFailure): void {
    this.end()
    const shown = failureOf(failure)
    if (!CONNECT_AGAIN[failure.kind]) {
      return this.giveUp({ kind: 'failed', cause: { kind: 'refused', failure: shown } })
    }
    const delay = this.delays[this.lost]
    this.lost++
    if (delay === undefined) {
      return this.giveUp({ kind: 'failed', cause: { kind: 'unreachable', failure: shown } })
    }
    this.report({ kind: 'reconnecting', failure: shown })
    // A plain one-shot timer per reconnect, which no Signality composable covers.
    const timer = setTimeout(() => {
      if (this.phase.kind === 'waiting' && this.phase.timer === timer) this.open()
    }, delay)
    this.phase = { kind: 'waiting', timer }
  }

  private giveUp(status: Extract<SourceStatus, { kind: 'failed' }>): void {
    this.phase = { kind: 'failed' }
    this.report(status)
    this.release()
  }

  /** Retires the current stream or the pending reconnect. */
  private end(): void {
    const phase = this.phase
    switch (phase.kind) {
      case 'streaming':
        clearTimeout(phase.deadline)
        phase.stream.close()
        break
      case 'paired':
        phase.stream.close()
        break
      case 'waiting':
        clearTimeout(phase.timer)
        break
      case 'idle':
      case 'failed':
      case 'stopped':
        return
    }
    this.phase = { kind: 'idle' }
  }

  private release(): void {
    for (const resolve of this.waiters.splice(0)) resolve()
  }

  private report(status: SourceStatus): void {
    this.sink?.connection(status)
  }
}
