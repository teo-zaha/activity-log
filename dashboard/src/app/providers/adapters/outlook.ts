import type { OutlookEntry } from '../../generated/activity'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'
import { createBadge, formatDuration, joinParts, toDefinedStrings } from '../utils'

class OutlookEntryUiAdapter extends EntryUiAdapter<OutlookEntry> {
  constructor(provider: OutlookProviderAdapter, entry: OutlookEntry) {
    const entryIcon = 'event'
    const badge = createBadge(provider.source, provider.label, entryIcon)

    super(provider, entry, {
      title: entry.title ?? '',
      meta: joinParts([
        formatDuration(entry.durationMinutes),
        entry.location,
        entry.isOrganizer ? 'organizer' : 'attendee',
        entry.organizer && !entry.isOrganizer ? `org: ${entry.organizer}` : '',
      ]),
      entryIcon,
      searchTerms: toDefinedStrings([entry.title, entry.location, entry.organizer]),
      calendarDurationMinutes: entry.durationMinutes,
      detailFields: [
        { label: 'Title', value: entry.title },
        { label: 'Duration', value: formatDuration(entry.durationMinutes) },
        { label: 'Location', value: entry.location ?? '' },
        { label: 'Organizer', value: entry.organizer ?? '' },
        { label: 'Role', value: entry.isOrganizer ? 'Organizer' : 'Attendee' },
      ].filter(field => field.value),
      badge,
      badges: [badge],
      detailSections: [],
      styleKey: provider.source,
      sources: [provider.source],
    })
  }

  markdownSummary(): string {
    const entry = this.entry
    return `${entry.title}${entry.location ? ` @ ${entry.location}` : ''}`
  }

  markdownMeta(timeLabel: string): string {
    const entry = this.entry
    return joinParts([timeLabel, formatDuration(entry.durationMinutes)])
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
