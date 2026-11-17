import type { CalendarEntry } from '../../generated/activity'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'
import { createBadge, formatDuration, joinParts, toDefinedStrings } from '../utils'

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
      searchTerms: toDefinedStrings([entry.title, entry.location, entry.organizer]),
      calendarDurationMinutes: entry.durationMinutes,
      detailFields: [
        { label: 'Title', value: entry.title },
        { label: 'Duration', value: formatDuration(entry.durationMinutes) },
        { label: 'Location', value: entry.location ?? '' },
        { label: 'Organizer', value: entry.organizer ?? '' },
      ].filter(field => field.value),
      badge,
      badges: [badge],
      detailSections: [],
      styleKey: provider.source,
    })
  }

  markdownSummary(): string {
    const entry = this.entry
    return `${entry.title}${entry.location ? ` @ ${entry.location}` : ''}`
  }

  markdownMeta(timeLabel: string): string {
    return joinParts([timeLabel, formatDuration(this.entry.durationMinutes)])
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
