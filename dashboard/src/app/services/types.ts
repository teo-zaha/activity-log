import type { ProviderInfo, Source } from '../generated/activity'
import type { Failure } from './failure'
import type { ConnectionKey } from '../providers/connection'

export interface ProviderViewModel {
  info: ProviderInfo
  key: ConnectionKey
  /** The connection's label when its kind has several, else the kind's name. */
  label: string
  /** The kind, for icons and colours. */
  chipKey: Source
  tooltip: string | null
  count: number
  /** The running collection still waits for this connection, so `count` is not known yet. */
  collecting: boolean
}

export type CollectionOperation = 'initial' | 'collect'

/** Lifecycle of the most recent collection request, as shown in the shell. */
export type CollectionStatus =
  | { readonly kind: 'idle' }
  | {
      readonly kind: 'collecting'
      readonly operation: CollectionOperation
      readonly startedAt: number
    }
  | {
      readonly kind: 'failed'
      readonly operation: CollectionOperation
      readonly origin: FailureOrigin
      readonly failure: Failure
    }

/**
 * Where a failure the shell shows comes from: the collect request the backend refused, the
 * backend lost after connections kept failing, a connection it refuses for good, or the
 * collection itself.
 */
export type FailureOrigin = 'collect' | 'lostBackend' | 'refusedRead' | 'collection'

/**
 * Where a save's install receipt stands against the state on screen (`receiptStanding`): its
 * host with nothing later installed (or no host on screen yet), its host with a later providers
 * revision installed, another host, or no receipt at all: the file was written but the host shut
 * down before it installed it, so no host on screen will show it.
 */
export type ReceiptStanding = 'current' | 'superseded' | 'otherHost' | 'notInstalled'
