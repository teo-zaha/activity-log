import type { JiraEntry } from '../../generated/activity'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'
import { createBadge, joinParts, toDefinedStrings } from '../utils'

class JiraEntryUiAdapter extends EntryUiAdapter<JiraEntry> {
  constructor(provider: JiraProviderAdapter, entry: JiraEntry) {
    const entryIcon = 'task_alt'
    const badge = createBadge(provider.source, provider.label, entryIcon)

    super(provider, entry, {
      title: entry.key ? `${entry.key}: ${entry.title ?? ''}` : (entry.title ?? ''),
      meta: joinParts([entry.issueType, entry.status]),
      entryIcon,
      searchTerms: toDefinedStrings([
        entry.key,
        entry.title,
        entry.status,
        entry.issueType,
        entry.type,
        entry.fromStatus,
        entry.toStatus,
      ]),
      calendarDurationMinutes: null,
      detailFields: [
        { label: 'Event', value: entry.type },
        { label: 'Key', value: entry.key },
        { label: 'Title', value: entry.title },
        { label: 'Status', value: entry.status },
        { label: 'Issue type', value: entry.issueType },
        { label: 'From status', value: entry.fromStatus ?? '' },
        { label: 'To status', value: entry.toStatus ?? '' },
      ].filter(field => field.value),
      badge,
      badges: [badge],
      detailSections: [],
      styleKey: provider.source,
    })
  }
}

class JiraProviderAdapter extends ProviderUiAdapterDefinition<
  'jira',
  JiraEntry,
  JiraEntryUiAdapter
> {
  readonly source = 'jira' as const
  readonly label = 'Jira'
  readonly providerIcon = 'task_alt'

  create(entry: JiraEntry): JiraEntryUiAdapter {
    return new JiraEntryUiAdapter(this, entry)
  }
}

export const jiraAdapter = new JiraProviderAdapter()
