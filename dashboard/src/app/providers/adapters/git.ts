import type { GitEntry } from '../../generated/activity'
import {
  EntryUiAdapter,
  ProviderUiAdapterDefinition,
  type EntryDetailSection,
  type ProviderDetailField,
} from '../types'
import { createBadge, joinParts, toDefinedStrings } from '../utils'
import { gitlabAdapter } from './gitlab'

class GitEntryUiAdapter extends EntryUiAdapter<GitEntry> {
  constructor(provider: GitProviderAdapter, entry: GitEntry) {
    const detailFields: ProviderDetailField[] = [
      { label: 'Repository', value: entry.repo },
      { label: 'Branch', value: entry.branch },
      { label: 'Commit', value: entry.hash },
      { label: 'Author', value: entry.author },
      { label: 'Message', value: entry.message },
      { label: 'GitLab event', value: entry.gitlabEvent?.title ?? '' },
    ].filter(field => field.value)
    const mergedGitlabEntry = entry.gitlabEvent ? gitlabAdapter.create(entry.gitlabEvent) : null
    const detailSections: EntryDetailSection[] =
      mergedGitlabEntry == null
        ? []
        : [
            {
              source: provider.source,
              title: provider.label,
              icon: provider.providerIcon,
              badgeClass: `badge-${provider.source}`,
              fields: detailFields,
            },
            {
              source: gitlabAdapter.source,
              title: gitlabAdapter.label,
              icon: gitlabAdapter.providerIcon,
              badgeClass: `badge-${gitlabAdapter.source}`,
              fields: mergedGitlabEntry.detailFields,
            },
          ]
    const badge = entry.gitlabEvent
      ? createBadge(provider.source, 'git/lab', 'commit', 'badge-gitab')
      : createBadge(provider.source, provider.label, 'commit')
    const badges = entry.gitlabEvent
      ? [
          createBadge(provider.source, provider.label, provider.providerIcon),
          createBadge(gitlabAdapter.source, gitlabAdapter.label, gitlabAdapter.providerIcon),
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
        ...(mergedGitlabEntry?.searchTerms ?? []),
      ]),
      calendarDurationMinutes: null,
      detailFields,
      badge,
      badges,
      detailSections,
      styleKey: entry.gitlabEvent ? 'gitab' : provider.source,
    })
  }

  markdownSummary(): string {
    const entry = this.entry
    return `\`${entry.repo}\` ${entry.message}`
  }

  markdownMeta(timeLabel: string): string {
    return timeLabel
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
