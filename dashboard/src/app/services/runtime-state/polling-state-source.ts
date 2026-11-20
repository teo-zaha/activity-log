import { DestroyRef, InjectionToken, Service, inject } from '@angular/core'
import type { RuntimeState } from '../../generated/activity'
import { ActivityBackendClient } from '../backend/backend-client'
import { BackendFailure, type BackendFailureKind } from '../backend/backend-failure'
import { type Failure, failureOf } from '../failure'
import type { RuntimeStateSink, RuntimeStateSource, SourceStatus } from './runtime-state-source'

/**
 * How many reads in a row may fail, and how many the loop makes while it follows one collection
 * or check, before it gives up.
 */
export const ACTIVITY_POLL_ATTEMPTS = new InjectionToken<number>('ACTIVITY_POLL_ATTEMPTS', {
  providedIn: 'root',
  // The backend gives up on a startup check after 15 s and on a collection after 120 s.
  factory: (): number => 180,
})

export const ACTIVITY_POLL_DELAY_MS = new InjectionToken<number>('ACTIVITY_POLL_DELAY_MS', {
  providedIn: 'root',
  factory: (): number => 1000,
})

/** How long one read may take before it is aborted and counts as failed. */
export const ACTIVITY_READ_TIMEOUT_MS = 10_000

/** Whether reading again could get the state, by how the backend failed the read. */
const READ_AGAIN: Readonly<Record<BackendFailureKind, boolean>> = {
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

/** A read failure no further read of this attempt can fix. */
function endsAttempt(failure: Failure): boolean {
  return failure.kind === 'backend' && !READ_AGAIN[failure.cause.kind]
}

/**
 * Reads the host's state (`get_runtime_state`, both parts from one moment with the host's own
 * stamps) while it does something: one loop, a read each second, as long as a collection runs
 * or a source is being checked. A command the backend answered has it read at once. Retry
 * starts a new attempt: the read of the old one is aborted and delivers nothing.
 */
@Service()
export class PollingStateSource implements RuntimeStateSource {
  private readonly backend = inject(ActivityBackendClient)
  private readonly attempts = inject(ACTIVITY_POLL_ATTEMPTS)
  private readonly delayMs = inject(ACTIVITY_POLL_DELAY_MS)

  private sink: RuntimeStateSink | null = null
  private stopped = false
  /** Numbers the connection attempts; a read of an older one delivers nothing. */
  private attempt = 0
  /** The next pair is the first of its attempt. */
  private attemptStarts = true
  /** A read was asked for: no pause before it. */
  private due = false
  /** Reads since the loop was last woken, while it follows something. */
  private reads = 0
  /** Failed reads in a row, and the last one's failure. */
  private failures = 0
  private failure: Failure | null = null
  private gaveUp = false
  private last: RuntimeState | null = null
  private loop: Promise<void> | null = null
  private endPause: (() => void) | null = null
  private inFlight: AbortController | null = null

  constructor() {
    inject(DestroyRef).onDestroy(() => {
      this.stopped = true
      this.sink = null
      this.inFlight?.abort()
      this.endPause?.()
    })
  }

  connect(sink: RuntimeStateSink): void {
    if (this.sink !== null || this.stopped)
      throw new Error('The state source cannot connect again.')
    this.sink = sink
    this.restart()
  }

  follow(): void {
    if (this.sink === null) return
    if (this.gaveUp) this.restart()
    else this.wake()
  }

  retry(): void {
    if (this.sink !== null) this.restart()
  }

  settled(): Promise<void> {
    return this.loop ?? Promise.resolve()
  }

  /** A new attempt: its first pair may bring another host, and the old one's read is dropped. */
  private restart(): void {
    this.attempt++
    this.attemptStarts = true
    this.inFlight?.abort()
    this.failures = 0
    this.failure = null
    this.report({ kind: 'connecting' })
    this.wake()
  }

  private wake(): void {
    this.due = true
    this.reads = 0
    this.endPause?.()
    // Started after this turn, so the loop can only end inside `run`, where it clears `loop` in
    // the same turn as it finds nothing left to read.
    this.loop ??= Promise.resolve().then(() => this.run())
  }

  private async run(): Promise<void> {
    for (;;) {
      if (this.stopped) return this.end()
      if (!this.due) {
        if (this.failure !== null && this.failures >= this.attempts) {
          this.report({ kind: 'failed', cause: { kind: 'unreachable', failure: this.failure } })
          return this.end()
        }
        if (this.failure === null && !this.following()) return this.end()
        if (this.failure === null && this.reads >= this.attempts) {
          this.report({ kind: 'failed', cause: { kind: 'timedOut' } })
          return this.end()
        }
        await this.pause()
        if (this.stopped) return this.end()
      }
      this.due = false
      this.reads++
      const attempt = this.attempt
      const read = await this.read()
      // Torn down, or retired by `retry`, while it was out: it delivers nothing, and the loop
      // ends or the new attempt reads next.
      if (this.stopped || attempt !== this.attempt) continue
      if (read.ok) {
        this.failures = 0
        this.failure = null
        this.last = read.state
        const authoritative = this.attemptStarts
        this.attemptStarts = false
        this.sink?.state(read.state, authoritative)
        // Taking the pair may have retired this attempt (a handshake asked for): the new one
        // reports for itself.
        if (attempt !== this.attempt) continue
        this.report({ kind: 'open' })
      } else if (endsAttempt(read.failure)) {
        this.report({ kind: 'failed', cause: { kind: 'refused', failure: read.failure } })
        return this.end()
      } else {
        this.failures++
        this.failure = read.failure
        this.report({ kind: 'reconnecting', failure: read.failure })
      }
    }
  }

  /** One read, aborted after `ACTIVITY_READ_TIMEOUT_MS`, by `retry` and by teardown. */
  private async read(): Promise<
    | { readonly ok: true; readonly state: RuntimeState }
    | { readonly ok: false; readonly failure: Failure }
  > {
    const controller = new AbortController()
    this.inFlight = controller
    // A plain timer: one-shot per read, which no Signality composable covers.
    const timer = setTimeout(
      () =>
        controller.abort(
          new BackendFailure(
            'unreachable',
            `The backend did not answer within ${ACTIVITY_READ_TIMEOUT_MS / 1000} s`
          )
        ),
      ACTIVITY_READ_TIMEOUT_MS
    )
    try {
      return { ok: true, state: await this.backend.getRuntimeState(controller.signal) }
    } catch (error) {
      return { ok: false, failure: failureOf(error) }
    } finally {
      clearTimeout(timer)
      if (this.inFlight === controller) this.inFlight = null
    }
  }

  private end(): void {
    this.loop = null
  }

  /** Whether the host is still doing something the dashboard shows. */
  private following(): boolean {
    const last = this.last
    if (last === null) return true
    const providers = last.providers.providers.providers
    if (providers.some(provider => provider.status === 'checking')) return true
    switch (last.activity.state.kind) {
      // Nothing is collected without a source; with one, the first collection is on its way.
      case 'empty':
        return providers.length > 0
      case 'running':
        return true
      case 'ready':
      case 'failed':
        return false
    }
  }

  private report(status: SourceStatus): void {
    this.gaveUp = status.kind === 'failed'
    this.sink?.connection(status)
  }

  /**
   * Waits between reads, or less once a read is asked for. A plain timer: it is one-shot and
   * awaited inside the loop, which Signality 0.5's `interval` (a repeating timer driven by a
   * signal) does not fit.
   */
  private pause(): Promise<void> {
    return new Promise(resolve => {
      const done = (): void => {
        clearTimeout(timer)
        this.endPause = null
        resolve()
      }
      const timer = setTimeout(done, this.delayMs)
      this.endPause = done
    })
  }
}
