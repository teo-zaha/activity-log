import type {
  ActivityEntry,
  CalendarEntry,
  GitEntry,
  GitLabEntry,
  JiraEntry,
  OutlookEntry,
  Source,
  SystemEntry,
} from '../generated/activity'
import { calendarAdapter } from './adapters/calendar'
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
  calendar: calendarAdapter,
}

export function getProviderAdapter<S extends Source>(source: S): ProviderAdapterMap[S] {
  return providerAdapters[source]
}

export function createEntryAdapter(entry: GitEntry): EntryUiAdapter<GitEntry>
export function createEntryAdapter(entry: GitLabEntry): EntryUiAdapter<GitLabEntry>
export function createEntryAdapter(entry: JiraEntry): EntryUiAdapter<JiraEntry>
export function createEntryAdapter(entry: OutlookEntry): EntryUiAdapter<OutlookEntry>
export function createEntryAdapter(entry: SystemEntry): EntryUiAdapter<SystemEntry>
export function createEntryAdapter(entry: CalendarEntry): EntryUiAdapter<CalendarEntry>
export function createEntryAdapter(entry: ActivityEntry): EntryUiAdapter
export function createEntryAdapter(entry: ActivityEntry): EntryUiAdapter {
  switch (entry.source) {
    case 'git':
      return gitAdapter.create(entry)
    case 'gitlab':
      return gitlabAdapter.create(entry)
    case 'jira':
      return jiraAdapter.create(entry)
    case 'outlook':
      return outlookAdapter.create(entry)
    case 'system':
      return systemAdapter.create(entry)
    case 'calendar':
      return calendarAdapter.create(entry)
  }
}
