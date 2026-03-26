import type { SystemEntry, SystemEntryType } from '../../generated/activity'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'
import { createBadge, toDefinedStrings } from '../utils'

const systemIcons: Record<SystemEntryType, string> = {
  boot: 'power_settings_new',
  shutdown: 'power_off',
  unexpected_shutdown: 'warning',
  sleep: 'bedtime',
  wake: 'wb_sunny',
  lock: 'lock',
  unlock: 'lock_open',
}

class SystemEntryUiAdapter extends EntryUiAdapter<SystemEntry> {
  constructor(provider: SystemProviderAdapter, entry: SystemEntry) {
    const entryIcon = systemIcons[entry.type] ?? 'power_settings_new'
    const badge = createBadge(provider.source, provider.label, entryIcon)

    super(provider, entry, {
      title: entry.label ?? entry.type,
      meta: entry.type,
      entryIcon,
      searchTerms: toDefinedStrings([entry.label, entry.type]),
      calendarDurationMinutes: null,
      detailFields: [
        { label: 'Event type', value: entry.type },
        { label: 'Label', value: entry.label },
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
    return entry.label
  }

  markdownMeta(timeLabel: string): string {
    return timeLabel
  }
}

class SystemProviderAdapter extends ProviderUiAdapterDefinition<
  'system',
  SystemEntry,
  SystemEntryUiAdapter
> {
  readonly source = 'system' as const
  readonly label = 'System'
  readonly providerIcon = 'power_settings_new'

  create(entry: SystemEntry): SystemEntryUiAdapter {
    return new SystemEntryUiAdapter(this, entry)
  }
}

export const systemAdapter = new SystemProviderAdapter()
