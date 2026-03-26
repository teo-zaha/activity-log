import type { ActivityEntry, Source } from '../generated/activity'
import { gitAdapter } from './adapters/git'
import { gitlabAdapter } from './adapters/gitlab'
import { jiraAdapter } from './adapters/jira'
import { outlookAdapter } from './adapters/outlook'
import { systemAdapter } from './adapters/system'
import type { EntryBySource, EntryUiAdapter, ProviderUiAdapterDefinition } from './types'

type ProviderAdapterMap = {
  [S in Source]: ProviderUiAdapterDefinition<S, EntryBySource<S>>
}

const providerAdapters: ProviderAdapterMap = {
  git: gitAdapter,
  gitlab: gitlabAdapter,
  jira: jiraAdapter,
  outlook: outlookAdapter,
  system: systemAdapter,
}

export function getProviderAdapter<S extends Source>(source: S): ProviderAdapterMap[S] {
  return providerAdapters[source]
}

export function createEntryAdapter<TEntry extends ActivityEntry>(
  entry: TEntry
): EntryUiAdapter<TEntry> {
  switch (entry.source) {
    case 'git':
      return gitAdapter.create(entry) as EntryUiAdapter<TEntry>
    case 'gitlab':
      return gitlabAdapter.create(entry) as EntryUiAdapter<TEntry>
    case 'jira':
      return jiraAdapter.create(entry) as EntryUiAdapter<TEntry>
    case 'outlook':
      return outlookAdapter.create(entry) as EntryUiAdapter<TEntry>
    case 'system':
      return systemAdapter.create(entry) as EntryUiAdapter<TEntry>
  }
}
