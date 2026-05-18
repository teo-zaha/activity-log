import type { ProviderInfo, Source } from '../../generated/activity'
import type { Theme } from '../../services/theme.service'

export type SettingsSelection = 'general' | `source:${Source}`

export type FutureProviderId = 'figma' | 'miro' | 'teams' | 'github'

export interface SettingsFutureProviderRow {
  readonly id: FutureProviderId
  readonly label: string
  readonly iconUrl: string
  readonly tooltip: string
}

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
