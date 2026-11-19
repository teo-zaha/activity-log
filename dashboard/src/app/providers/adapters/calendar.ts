import type { CalendarEntry } from '../../generated/activity'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'
import { busyStatusLabel, createBadge, formatDuration, joinParts, toDefinedStrings } from '../utils'

class CalendarEntryUiAdapter extends EntryUiAdapter<CalendarEntry> {
  constructor(provider: CalendarProviderAdapter, entry: CalendarEntry) {
    const entryIcon = 'event'
    const badge = createBadge(provider.source, provider.label, entryIcon)

    super(provider, entry, {
      title: entry.title,
      meta: joinParts([
        formatDuration(entry.durationMinutes),
        entry.location,
        entry.organizer ? `org: ${entry.organizer}` : '',
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
        { label: 'Title', value: entry.title },
        { label: 'Duration', value: formatDuration(entry.durationMinutes) },
        { label: 'Status', value: entry.busyStatus ? busyStatusLabel(entry.busyStatus) : '' },
        { label: 'Location', value: entry.location ?? '' },
        { label: 'Organizer', value: entry.organizer ?? '' },
      ].filter(field => field.value),
      badge,
      badges: [badge],
      detailSections: [],
      styleKey: provider.source,
    })
  }
}

class CalendarProviderAdapter extends ProviderUiAdapterDefinition<
  'calendar',
  CalendarEntry,
  CalendarEntryUiAdapter
> {
  readonly source = 'calendar' as const
  readonly label = 'Calendar'
  readonly providerIcon = 'calendar_month'

  create(entry: CalendarEntry): CalendarEntryUiAdapter {
    return new CalendarEntryUiAdapter(this, entry)
  }
}

export const calendarAdapter = new CalendarProviderAdapter()
