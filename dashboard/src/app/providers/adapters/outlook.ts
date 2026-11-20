import type { OutlookEntry } from '../../generated/activity'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'
import {
  NO_TITLE,
  busyStatusLabel,
  createBadge,
  formatDuration,
  joinParts,
  toDefinedStrings,
} from '../utils'

/** The user's role in the event; none when Outlook did not say. */
function roleOf(entry: OutlookEntry): 'Organizer' | 'Attendee' | null {
  if (entry.isOrganizer === undefined) return null
  return entry.isOrganizer ? 'Organizer' : 'Attendee'
}

class OutlookEntryUiAdapter extends EntryUiAdapter<OutlookEntry> {
  constructor(provider: OutlookProviderAdapter, entry: OutlookEntry) {
    const entryIcon = 'event'
    const role = roleOf(entry)
    const badge = createBadge(provider.source, provider.label, entryIcon)

    super(provider, entry, {
      title: entry.title ?? NO_TITLE,
      meta: joinParts([
        formatDuration(entry.durationMinutes),
        entry.location,
        role?.toLowerCase(),
        entry.organizer && role !== 'Organizer' ? `org: ${entry.organizer}` : '',
      ]),
      entryIcon,
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
        { label: 'Title', value: entry.title ?? NO_TITLE },
        { label: 'Duration', value: formatDuration(entry.durationMinutes) },
        { label: 'Status', value: entry.busyStatus ? busyStatusLabel(entry.busyStatus) : '' },
        { label: 'Location', value: entry.location ?? '' },
        { label: 'Organizer', value: entry.organizer ?? '' },
        { label: 'Role', value: role ?? '' },
      ].filter(field => field.value),
      badge,
      badges: [badge],
      detailSections: [],
      styleKey: provider.source,
    })
  }
}

class OutlookProviderAdapter extends ProviderUiAdapterDefinition<
  'outlook',
  OutlookEntry,
  OutlookEntryUiAdapter
> {
  readonly source = 'outlook' as const
  readonly label = 'Outlook'
  readonly providerIcon = 'event'

  create(entry: OutlookEntry): OutlookEntryUiAdapter {
    return new OutlookEntryUiAdapter(this, entry)
  }
}

export const outlookAdapter = new OutlookProviderAdapter()
