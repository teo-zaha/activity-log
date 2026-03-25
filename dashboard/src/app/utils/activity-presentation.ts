import type { ActivityEntry, SystemEntryType } from '../generated/activity'

export function getEntryTitle(entry: ActivityEntry): string {
  switch (entry.source) {
    case 'git':
      return entry.message ?? ''
    case 'gitlab':
      return entry.title || entry.type
    case 'jira':
      return entry.key ? `${entry.key}: ${entry.title ?? ''}` : (entry.title ?? '')
    case 'outlook':
      return entry.title ?? ''
    case 'system':
      return entry.label ?? entry.type
  }
}

export function getEntryMeta(entry: ActivityEntry): string {
  switch (entry.source) {
    case 'git': {
      const parts = [entry.repo, entry.branch, entry.hash?.slice(0, 7)]
      if (entry.gitlabEvent) parts.push(entry.gitlabEvent.title)
      return parts.filter(Boolean).join(' · ')
    }
    case 'gitlab':
      return [entry.type, entry.target_type].filter(Boolean).join(' · ')
    case 'jira':
      return [entry.issue_type, entry.status].filter(Boolean).join(' · ')
    case 'outlook':
      return [
        formatDuration(entry.duration_minutes),
        entry.location,
        entry.is_organizer ? 'organizer' : 'attendee',
        entry.organizer && !entry.is_organizer ? `org: ${entry.organizer}` : '',
      ]
        .filter(Boolean)
        .join(' · ')
    case 'system':
      return entry.type
  }
}

export function getEntryIcon(entry: ActivityEntry): string {
  if (entry.source === 'system') {
    const icons: Record<SystemEntryType, string> = {
      boot: 'power_settings_new',
      shutdown: 'power_off',
      unexpected_shutdown: 'warning',
      sleep: 'bedtime',
      wake: 'wb_sunny',
      lock: 'lock',
      unlock: 'lock_open',
    }
    return icons[entry.type] ?? 'power_settings_new'
  }

  const icons: Record<ActivityEntry['source'], string> = {
    git: 'commit',
    gitlab: 'code',
    jira: 'task_alt',
    outlook: 'event',
    system: 'power_settings_new',
  }
  return icons[entry.source] ?? 'circle'
}

export function formatDuration(durationMinutes: number): string {
  if (durationMinutes >= 60) {
    const hours = Math.floor(durationMinutes / 60)
    const minutes = durationMinutes % 60
    return minutes ? `${hours}h ${minutes}m` : `${hours}h`
  }
  return `${durationMinutes}m`
}

export function getEntrySearchTerms(entry: ActivityEntry): (string | undefined)[] {
  switch (entry.source) {
    case 'git':
      return [entry.message, entry.branch, entry.repo]
    case 'gitlab':
      return [
        entry.title,
        entry.type,
        entry.target_type,
        entry.branch,
        entry.note_body,
        entry.note_file,
      ]
    case 'jira':
      return [entry.key, entry.title, entry.status, entry.issue_type, entry.type, entry.from_status, entry.to_status]
    case 'outlook':
      return [entry.title, entry.location, entry.organizer]
    case 'system':
      return [entry.label, entry.type]
  }
}
