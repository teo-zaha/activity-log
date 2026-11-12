import type { ProviderInfo, Source } from '../generated/activity'

export type ViewMode = 'list' | 'day' | 'workweek' | 'week'

export interface ProviderViewModel {
  info: ProviderInfo
  label: string
  icon: string
  chipKey: Source
  tooltip: string | null
  count: number
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
