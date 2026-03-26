import type { Source } from '../generated/activity'
import type { EntryBadgeVm } from './types'

export function formatDuration(durationMinutes: number): string {
  if (durationMinutes >= 60) {
    const hours = Math.floor(durationMinutes / 60)
    const minutes = durationMinutes % 60
    return minutes ? `${hours}h ${minutes}m` : `${hours}h`
  }
  return `${durationMinutes}m`
}

export function joinParts(parts: Array<string | undefined | null | false>): string {
  return parts.filter(Boolean).join(' · ')
}

export function toDefinedStrings(values: Array<string | undefined | null | false>): string[] {
  return values.filter((value): value is string => Boolean(value))
}

export function createBadge(
  source: Source,
  label: string,
  icon: string,
  badgeClass = `badge-${source}`
): EntryBadgeVm {
  return { source, label, icon, badgeClass }
}
