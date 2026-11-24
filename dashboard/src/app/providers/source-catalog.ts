import type { SupportedOs } from '../generated/activity'

/** Sources that cannot be added yet; shown as disabled tiles. */
export interface UpcomingSource {
  readonly id: 'teams' | 'figma' | 'miro' | 'odoo'
  readonly label: string
  readonly iconUrl: string
}

export const UPCOMING_SOURCES: readonly UpcomingSource[] = [
  { id: 'teams', label: 'Microsoft Teams', iconUrl: 'assets/provider-icons/teams.svg' },
  { id: 'figma', label: 'Figma', iconUrl: 'assets/provider-icons/figma.svg' },
  { id: 'miro', label: 'Miro', iconUrl: 'assets/provider-icons/miro.svg' },
  { id: 'odoo', label: 'Odoo', iconUrl: 'assets/provider-icons/odoo.svg' },
]

const OS_LABELS: Readonly<Record<SupportedOs, string>> = {
  windows: 'Windows',
  macos: 'macOS',
  linux: 'Linux',
}

export function supportedOsLabel(os: SupportedOs): string {
  return OS_LABELS[os]
}

/** `Windows only`, `Windows and macOS only`. */
export function onlyOnLabel(systems: readonly SupportedOs[]): string {
  const names = systems.map(supportedOsLabel)
  const list =
    names.length > 1 ? `${names.slice(0, -1).join(', ')} and ${names[names.length - 1]}` : names[0]
  return `${list ?? 'No system'} only`
}
