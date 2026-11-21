import type { CalendarEntry, OutlookEntry } from '../generated/activity'
import type { EntryUiAdapterInit, ProviderUiAdapterDefinition } from './types'
import {
  NO_TITLE,
  busyStatusLabel,
  createBadge,
  formatDuration,
  joinParts,
  toDefinedStrings,
} from './utils'

/**
 * What every meeting source sends: the backend's `MeetingBody` (flattened into each entry, so it
 * has no generated type of its own) and the title, place and organizer around it.
 */
export type MeetingFields = Pick<
  OutlookEntry | CalendarEntry,
  | 'title'
  | 'location'
  | 'organizer'
  | 'durationMinutes'
  | 'busyStatus'
  | 'meetingWeight'
  | 'description'
  | 'descriptionBlocks'
  | 'joinUrl'
>

/** The user's part in a meeting, where the source says it. */
export type MeetingRole = 'Organizer' | 'Attendee'

const MEETING_ICON = 'event'

/**
 * How a meeting shows, whichever source it came from. The organizer is named unless it is the
 * user; the role shows where the source knows it.
 */
export function meetingInit(
  provider: Pick<ProviderUiAdapterDefinition, 'source' | 'label'>,
  entry: MeetingFields,
  role: MeetingRole | null
): EntryUiAdapterInit {
  const badge = createBadge(provider.source, provider.label, MEETING_ICON)
  const title = entry.title ?? NO_TITLE
  const duration = formatDuration(entry.durationMinutes)
  return {
    title,
    meta: joinParts([
      duration,
      entry.location,
      role?.toLowerCase(),
      entry.organizer && role !== 'Organizer' ? `org: ${entry.organizer}` : null,
    ]),
    entryIcon: MEETING_ICON,
    searchTerms: toDefinedStrings([
      entry.title,
      entry.location,
      entry.organizer,
      entry.description,
    ]),
    calendarDurationMinutes: entry.durationMinutes,
    busyStatus: entry.busyStatus ?? null,
    meetingWeight: entry.meetingWeight,
    description: entry.description ?? null,
    descriptionBlocks: entry.descriptionBlocks ?? [],
    joinUrl: entry.joinUrl ?? null,
    detailFields: [
      { label: 'Title', value: title },
      { label: 'Duration', value: duration },
      { label: 'Status', value: entry.busyStatus ? busyStatusLabel(entry.busyStatus) : '' },
      { label: 'Location', value: entry.location ?? '' },
      { label: 'Organizer', value: entry.organizer ?? '' },
      { label: 'Role', value: role ?? '' },
    ].filter(field => field.value),
    badge,
    badges: [badge],
    detailSections: [],
    styleKey: provider.source,
  }
}
