import type { ProviderInfo } from '../../generated/activity'
import type { ConnectionKey } from '../../providers/connection'

/**
 * The settings pages that are not a source. `home` is the phone's list of sections; wider
 * screens list them beside the page and show General for it.
 */
export const SETTINGS_PAGES = ['home', 'general', 'logs', 'add'] as const
export type SettingsPage = (typeof SETTINGS_PAGES)[number]

/** What the settings show: a page, or a source by its key. It is also the URL's `settings`. */
export type SettingsSelection = SettingsPage | ConnectionKey

export function isSettingsPage(selection: SettingsSelection): selection is SettingsPage {
  return SETTINGS_PAGES.some(page => page === selection)
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
