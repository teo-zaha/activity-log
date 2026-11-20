import type { BusyStatus, Source } from '../generated/activity'
import type { EntryBadgeVm } from './types'

/** What a meeting without a subject shows as its title. */
export const NO_TITLE = '(No title)'

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

export function createBadge(source: Source, label: string, icon: string): EntryBadgeVm {
  return { source, label, icon }
}

const BUSY_STATUS_LABELS: Record<BusyStatus, string> = {
  busy: 'Busy',
  tentative: 'Tentative',
  free: 'Free',
  out_of_office: 'Out of office',
  working_elsewhere: 'Working elsewhere',
}

export function busyStatusLabel(status: BusyStatus): string {
  return BUSY_STATUS_LABELS[status]
}

/** The meetings that get a marker in the timeline and calendar views. */
export type BusyMarker = 'tentative' | 'free'

export function busyMarkerOf(status: BusyStatus | null): BusyMarker | null {
  return status === 'tentative' || status === 'free' ? status : null
}
