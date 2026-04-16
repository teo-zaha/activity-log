import type { ProviderInfo, Source } from '../../generated/activity'
import type { Theme } from '../../services/theme.service'

export type SettingsSelection = 'general' | `source:${Source}`

export interface SettingsProviderRow {
  readonly info: ProviderInfo
  readonly label: string
  readonly icon: string
  readonly tooltip: string | null
}

export interface SettingsThemeOption {
  readonly id: Theme
  readonly label: string
}
