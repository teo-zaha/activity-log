import { inject, Service } from '@angular/core'
import type { ActivityUpdate, ProvidersUpdate, RuntimeState } from '../../generated/activity'
import type { Failure } from '../failure'
import { PushStateSource } from './push-state-source'

/** Why a source stopped trying until `retry()`. */
export type SourceFailure =
  /** Connections kept failing in a way another one could fix; the last one's failure. */
  | { readonly kind: 'unreachable'; readonly failure: Failure }
  /** The backend refused the connection in a way connecting again cannot fix. */
  | { readonly kind: 'refused'; readonly failure: Failure }

export type SourceStatus =
  | { readonly kind: 'connecting' }
  | { readonly kind: 'open' }
  /** The state shown stays while the source tries again. */
  | { readonly kind: 'reconnecting'; readonly failure: Failure }
  | { readonly kind: 'failed'; readonly cause: SourceFailure }

/**
 * Where a source delivers the host's state, stamped by the host
 * (`docs/plans/shared-contracts.md` §5); the coordinator decides by the stamps what applies.
 */
export interface RuntimeStateSink {
  /**
   * Both parts from one moment. `authoritative` marks the pair a connection attempt starts with,
   * the only one that may bring another host (another epoch).
   */
  state(pair: RuntimeState, authoritative: boolean): void
  activity(update: ActivityUpdate): void
  providers(update: ProvidersUpdate): void
  connection(status: SourceStatus): void
}

/**
 * How the host's collection and source state reaches the dashboard. The coordinator
 * (`ActivityDataService`) applies what the source delivers and sends the commands. The host
 * pushes it: `PushStateSource` follows `GET /api/events` or the desktop app's events.
 */
@Service({ factory: (): RuntimeStateSource => inject(PushStateSource) })
export abstract class RuntimeStateSource {
  /** Starts delivering to `sink`, once. */
  abstract connect(sink: RuntimeStateSink): void
  /**
   * The backend answered a command, so it is reachable: a source that gave up starts a new
   * connection attempt; one that is connected does nothing, the host tells it what changed.
   */
  abstract follow(): void
  /** A new connection attempt: after `failed`, or for a pair the coordinator missed. */
  abstract retry(): void
  /** Resolves once the current attempt brought its first pair, or the source gave up. */
  abstract settled(): Promise<void>
}
