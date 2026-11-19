import type { ProviderInfo, Source } from '../generated/activity'
import type { ConnectionKey } from '../providers/connection'

export const VIEW_MODES = ['list', 'day', 'workweek', 'week'] as const

export type ViewMode = (typeof VIEW_MODES)[number]

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
  | { readonly kind: 'timedOut'; readonly operation: CollectionOperation; readonly message: string }
  | { readonly kind: 'failed'; readonly operation: CollectionOperation; readonly message: string }
