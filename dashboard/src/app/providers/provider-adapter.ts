import type {
  ActivityEntry,
  CalendarEntry,
  FigmaEntry,
  GitEntry,
  GitHubEntry,
  GitLabEntry,
  JiraEntry,
  OutlookEntry,
  Source,
  SystemEntry,
} from '../generated/activity'
import { calendarAdapter } from './adapters/calendar'
import { figmaAdapter } from './adapters/figma'
import { gitAdapter } from './adapters/git'
import { githubAdapter } from './adapters/github'
import { gitlabAdapter } from './adapters/gitlab'
import { jiraAdapter } from './adapters/jira'
import { outlookAdapter } from './adapters/outlook'
import { systemAdapter } from './adapters/system'
import { jsonEqual } from '../utils/json-equal'
import {
  type EntryBySource,
  type EntryUiAdapter,
  type ProviderUiAdapterDefinition,
  entryTrackKey,
} from './types'

type ProviderAdapterMap = {
  [S in Source]: ProviderUiAdapterDefinition<S, EntryBySource<S>>
}

const providerAdapters: ProviderAdapterMap = {
  git: gitAdapter,
  gitlab: gitlabAdapter,
  github: githubAdapter,
  jira: jiraAdapter,
  outlook: outlookAdapter,
  system: systemAdapter,
  calendar: calendarAdapter,
  figma: figmaAdapter,
}

export function getProviderAdapter<S extends Source>(source: S): ProviderAdapterMap[S] {
  return providerAdapters[source]
}

export function createEntryAdapter(entry: GitEntry): EntryUiAdapter<GitEntry>
export function createEntryAdapter(entry: GitLabEntry): EntryUiAdapter<GitLabEntry>
export function createEntryAdapter(entry: GitHubEntry): EntryUiAdapter<GitHubEntry>
export function createEntryAdapter(entry: JiraEntry): EntryUiAdapter<JiraEntry>
export function createEntryAdapter(entry: OutlookEntry): EntryUiAdapter<OutlookEntry>
export function createEntryAdapter(entry: SystemEntry): EntryUiAdapter<SystemEntry>
export function createEntryAdapter(entry: CalendarEntry): EntryUiAdapter<CalendarEntry>
export function createEntryAdapter(entry: FigmaEntry): EntryUiAdapter<FigmaEntry>
export function createEntryAdapter(entry: ActivityEntry): EntryUiAdapter
export function createEntryAdapter(entry: ActivityEntry): EntryUiAdapter {
  switch (entry.source) {
    case 'git':
      return gitAdapter.create(entry)
    case 'gitlab':
      return gitlabAdapter.create(entry)
    case 'github':
      return githubAdapter.create(entry)
    case 'jira':
      return jiraAdapter.create(entry)
    case 'outlook':
      return outlookAdapter.create(entry)
    case 'system':
      return systemAdapter.create(entry)
    case 'calendar':
      return calendarAdapter.create(entry)
    case 'figma':
      return figmaAdapter.create(entry)
  }
}

/**
 * Adapters for `entries`, keeping those of `previous` whose entries came again unchanged: each
 * answer of a running collection repeats what is already shown, and kept adapters keep the
 * work done for them (and their list rows) as they were.
 */
export function createEntryAdapters(
  entries: readonly ActivityEntry[],
  previous: readonly EntryUiAdapter[]
): EntryUiAdapter[] {
  const kept = new Map(previous.map(adapter => [adapter.trackKey, adapter]))
  return entries.map(entry => {
    const adapter = kept.get(entryTrackKey(entry))
    return adapter && jsonEqual(adapter.entry, entry) ? adapter : createEntryAdapter(entry)
  })
}
