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
