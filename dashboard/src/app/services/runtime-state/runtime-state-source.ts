import { inject, Service } from '@angular/core'
import type { ActivityUpdate, ProvidersUpdate, RuntimeState } from '../../generated/activity'
import type { Failure } from '../failure'
import { PollingStateSource } from './polling-state-source'

/** Why a source stopped trying until `retry()`. */
export type SourceFailure =
  /** Reads kept failing in a way another read could fix; the last one's failure. */
  | { readonly kind: 'unreachable'; readonly failure: Failure }
  /** The backend refused the read in a way reading again cannot fix (a route it lacks). */
  | { readonly kind: 'refused'; readonly failure: Failure }
  /** A collection or a check went on for longer than the backend allows it. */
  | { readonly kind: 'timedOut' }

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
 * (`ActivityDataService`) applies what the source delivers and sends the commands. Today that
 * is `PollingStateSource`, which reads `get_runtime_state`; a pushing source delivers the same
 * pairs and updates.
 */
@Service({ factory: (): RuntimeStateSource => inject(PollingStateSource) })
export abstract class RuntimeStateSource {
  /** Starts delivering to `sink`, once. */
  abstract connect(sink: RuntimeStateSink): void
  /**
   * The backend answered a command: the host may collect or check now. A source that reads the
   * state reads it at once and follows what it shows; one that is told the state does nothing.
   */
  abstract follow(): void
  /** A new connection attempt: after `failed`, or for a pair the coordinator missed. */
  abstract retry(): void
  /** Resolves once nothing the backend does is being followed any more. */
  abstract settled(): Promise<void>
}
