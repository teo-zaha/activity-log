import type { GitEntry } from '../../generated/activity'
import {
  EntryUiAdapter,
  ProviderUiAdapterDefinition,
  type EntryDetailSection,
  type ProviderDetailField,
} from '../types'
import { createBadge, joinParts, toDefinedStrings } from '../utils'
import { githubAdapter } from './github'
import { gitlabAdapter } from './gitlab'

class GitEntryUiAdapter extends EntryUiAdapter<GitEntry> {
  constructor(provider: GitProviderAdapter, entry: GitEntry) {
    const detailFields: ProviderDetailField[] = [
      { label: 'Repository', value: entry.repo },
      { label: 'Branch', value: entry.branch ?? '' },
      { label: 'Commit', value: entry.hash },
      { label: 'Author', value: entry.author },
      { label: 'Message', value: entry.message },
      { label: 'GitLab event', value: entry.gitlabEvent?.title ?? '' },
    ].filter(field => field.value)
    const gitlab = entry.gitlabEvent ? gitlabAdapter.create(entry.gitlabEvent) : null
    const github = entry.githubEvent ? githubAdapter.create(entry.githubEvent) : null
    const hosts = [
      gitlab && { entry: gitlab, label: 'Lab' },
      github && { entry: github, label: 'Hub' },
    ].filter(host => host !== null)
    const detailSections: EntryDetailSection[] = hosts.length
      ? [
          {
            source: provider.source,
            title: provider.label,
            icon: provider.providerIcon,
            fields: detailFields,
          },
          ...hosts.map(({ entry }) => ({
            source: entry.source,
            title: entry.provider.label,
            icon: entry.provider.providerIcon,
            fields: entry.detailFields,
          })),
        ]
      : []
    const badge = createBadge(provider.source, provider.label, 'commit')
    if (hosts.length) {
      badge.splits = hosts.map(({ entry, label }) => ({ source: entry.source, label }))
    }
    const badges = hosts.length
      ? [
          createBadge(provider.source, provider.label, provider.providerIcon),
          ...hosts.map(({ entry }) => entry.badge),
        ]
      : [badge]

    super(provider, entry, {
      title: entry.message ?? '',
      meta: joinParts([
        entry.repo,
        entry.branch,
        entry.hash?.slice(0, 7),
        entry.gitlabEvent?.title,
      ]),
      entryIcon: 'commit',
      searchTerms: toDefinedStrings([
        entry.message,
        entry.branch,
        entry.repo,
        ...hosts.flatMap(({ entry }) => entry.searchTerms),
      ]),
      calendarDurationMinutes: null,
      detailFields,
      detailLink: github?.detailLink,
      badge,
      badges,
      detailSections,
      styleKey: [provider.source, ...hosts.map(({ entry }) => entry.source)].join(' '),
    })
  }
}

class GitProviderAdapter extends ProviderUiAdapterDefinition<'git', GitEntry, GitEntryUiAdapter> {
  readonly source = 'git' as const
  readonly label = 'Git'
  readonly providerIcon = 'commit'

  create(entry: GitEntry): GitEntryUiAdapter {
    return new GitEntryUiAdapter(this, entry)
  }
}

export const gitAdapter = new GitProviderAdapter()
