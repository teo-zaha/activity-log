import type { FigmaActivity, FigmaEditor, FigmaEntry } from '../../generated/activity'
import { EntryUiAdapter, ProviderUiAdapterDefinition, type EntryUiAdapterInit } from '../types'
import { createBadge, toDefinedStrings } from '../utils'

const editorLabels: Readonly<Record<FigmaEditor, string>> = {
  figma: 'Design',
  figjam: 'FigJam',
  slides: 'Slides',
  buzz: 'Buzz',
  sites: 'Sites',
  make: 'Make',
}

const NO_COMMENT = '(No comment text)'

function activityView(
  activity: FigmaActivity
): Pick<EntryUiAdapterInit, 'meta' | 'searchTerms' | 'detailFields'> {
  switch (activity.type) {
    case 'versionSaved': {
      const detailFields = [{ label: 'Version', value: activity.label }]
      if (activity.description) {
        detailFields.push({ label: 'Description', value: activity.description })
      }
      return {
        meta: `Saved version “${activity.label}”`,
        searchTerms: toDefinedStrings([activity.label, activity.description]),
        detailFields,
      }
    }
    case 'edited':
      return {
        meta: 'Edited',
        searchTerms: [],
        detailFields: [],
      }
    case 'commented':
      return {
        meta: 'Commented',
        searchTerms: toDefinedStrings([activity.message]),
        detailFields: [
          { label: 'Comment', value: activity.message ?? NO_COMMENT },
          { label: 'Resolved', value: activity.resolved ? 'Yes' : 'No' },
        ],
      }
    case 'replied':
      return {
        meta: 'Replied',
        searchTerms: toDefinedStrings([activity.message]),
        detailFields: [{ label: 'Comment', value: activity.message ?? NO_COMMENT }],
      }
  }
  const exhaustive: never = activity
  return exhaustive
}

class FigmaEntryUiAdapter extends EntryUiAdapter<FigmaEntry> {
  constructor(provider: FigmaProviderAdapter, entry: FigmaEntry) {
    const entryIcon = provider.providerIcon
    const badge = createBadge(provider.source, provider.label, entryIcon)
    const activity = activityView(entry)
    const fileFields = [{ label: 'File', value: entry.fileName }]
    if (entry.editor) {
      fileFields.push({ label: 'Kind', value: editorLabels[entry.editor] })
    }

    super(provider, entry, {
      title: entry.fileName,
      meta: activity.meta,
      entryIcon,
      searchTerms: [entry.fileName, activity.meta, ...activity.searchTerms],
      calendarDurationMinutes: null,
      detailFields: [...fileFields, ...activity.detailFields],
      detailLink: { label: 'Open in Figma', url: entry.fileUrl },
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
