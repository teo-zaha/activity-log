import type { FigmaActivity, FigmaEntry } from '../../generated/activity'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'
import { createBadge } from '../utils'

const actions: Readonly<Record<FigmaActivity['type'], string>> = {
  versionSaved: 'Saved a version',
  edited: 'Edited',
  commented: 'Commented',
  replied: 'Replied',
}

class FigmaEntryUiAdapter extends EntryUiAdapter<FigmaEntry> {
  constructor(provider: FigmaProviderAdapter, entry: FigmaEntry) {
    const entryIcon = provider.providerIcon
    const badge = createBadge(provider.source, provider.label, entryIcon)
    const meta = actions[entry.type]

    super(provider, entry, {
      title: entry.fileName,
      meta,
      entryIcon,
      searchTerms: [entry.fileName, meta],
      calendarDurationMinutes: null,
      detailFields: [],
      badge,
      badges: [badge],
      detailSections: [],
      styleKey: provider.source,
    })
  }
}

class FigmaProviderAdapter extends ProviderUiAdapterDefinition<
  'figma',
  FigmaEntry,
  FigmaEntryUiAdapter
> {
  readonly source = 'figma' as const
  readonly label = 'Figma'
  readonly providerIcon = 'draw'

  create(entry: FigmaEntry): FigmaEntryUiAdapter {
    return new FigmaEntryUiAdapter(this, entry)
  }
}

export const figmaAdapter = new FigmaProviderAdapter()
