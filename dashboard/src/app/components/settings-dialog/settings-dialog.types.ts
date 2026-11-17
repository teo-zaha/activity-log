import type { ProviderInfo } from '../../generated/activity'
import type { ConnectionKey } from '../../providers/connection'
import type { Theme } from '../../services/theme.service'

export type SettingsSelection = 'general' | 'logs' | 'add' | `connection:${ConnectionKey}`

export function connectionSelection(key: ConnectionKey): SettingsSelection {
  return `connection:${key}`
}

/** What the settings dialog opens on, besides General. */
export type SettingsTarget = ConnectionKey | 'add'

/** The settings sections that are not sources, in the order the navigation lists them. */
export const SETTINGS_SECTIONS = [
  { id: 'general', label: 'General', icon: 'tune' },
  { id: 'logs', label: 'Logs', icon: 'receipt_long' },
] as const satisfies readonly { id: SettingsSelection; label: string; icon: string }[]

export interface SettingsProviderRow {
  readonly info: ProviderInfo
  readonly key: ConnectionKey
  /** The connection's label when its kind has several, else the kind's name. */
  readonly label: string
  /** The kind's name, such as `GitLab`. */
  readonly kindLabel: string
  readonly tooltip: string | null
}

export interface SettingsThemeOption {
  readonly id: Theme
  readonly label: string
}

export interface SettingsDialogData {
  readonly initialSelection: SettingsSelection
}
