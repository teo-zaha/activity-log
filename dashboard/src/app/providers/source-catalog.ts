import type { Source, SupportedOs } from '../generated/activity'

/** What a source collects, shown on its tile when adding one. */
export const SOURCE_DESCRIPTIONS: Readonly<Record<Source, string>> = {
  git: 'Commits from your repositories',
  gitlab: 'Merge requests, pushes, comments and reviews',
  jira: 'Issues assigned to or reported by you',
  outlook: 'Meetings from the Outlook desktop app',
  calendar: 'Events from an ICS calendar link or .ics file',
  system: 'When this computer started and shut down',
}

/** Sources that cannot be added yet; shown as disabled tiles. */
export interface UpcomingSource {
  readonly id: 'github' | 'teams' | 'figma' | 'miro' | 'odoo'
  readonly label: string
  readonly iconUrl: string
}

export const UPCOMING_SOURCES: readonly UpcomingSource[] = [
  { id: 'github', label: 'GitHub', iconUrl: 'assets/provider-icons/github.svg' },
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
