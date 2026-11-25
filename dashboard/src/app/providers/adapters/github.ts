import type { GitHubEntry, PullRequestRef } from '../../generated/activity'
import { changeReference, changeStackVmOf } from '../change-stack'
import { CODE_HOST_REFERENCE_PREFIX, githubAction } from '../code-host'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'
import { createBadge, joinParts, toDefinedStrings } from '../utils'

function pullDetails(pull: PullRequestRef) {
  return {
    target: pull,
    itemLabel: 'PR',
    referencePrefix: CODE_HOST_REFERENCE_PREFIX.pullRequest,
    linkLabel: 'Open pull request',
    entryIcon: 'code',
    changeStack: pull.changeStack
      ? changeStackVmOf(pull.changeStack, CODE_HOST_REFERENCE_PREFIX.pullRequest)
      : null,
  }
}

function detailsOf(entry: Exclude<GitHubEntry, { type: 'committed' }>) {
  switch (entry.type) {
    case 'opened':
      return entry.target.kind === 'pullRequest'
        ? pullDetails(entry.target)
        : {
            target: entry.target,
            itemLabel: 'Issue',
            referencePrefix: CODE_HOST_REFERENCE_PREFIX.issue,
            linkLabel: 'Open issue',
            entryIcon: 'adjust',
            changeStack: null,
          }
    case 'reviewed':
      return pullDetails(entry.pullRequest)
  }
}

class GitHubEntryUiAdapter extends EntryUiAdapter<GitHubEntry> {
  constructor(provider: GitHubProviderAdapter, entry: GitHubEntry) {
    const action = githubAction(entry)
    if (entry.type === 'committed') {
      const badge = createBadge(provider.source, provider.label, 'commit')
      super(provider, entry, {
        title: entry.message ?? 'Commit',
        meta: joinParts([entry.repository, action.summary, entry.sha.slice(0, 7)]),
        entryIcon: 'commit',
        searchTerms: toDefinedStrings([entry.message, entry.repository, action.summary, entry.sha]),
        calendarDurationMinutes: null,
        detailLink: entry.webUrl ? { label: 'Open commit', url: entry.webUrl } : null,
        detailFields: [
          { label: 'Action', value: action.detail },
          { label: 'Repository', value: entry.repository },
          { label: 'Commit', value: entry.sha },
        ],
        badge,
        badges: [badge],
        detailSections: [],
        styleKey: provider.source,
      })
      return
    }
    const { target, itemLabel, referencePrefix, linkLabel, entryIcon, changeStack } =
      detailsOf(entry)
    const number = changeReference(referencePrefix, target.number)
    const badge = createBadge(provider.source, provider.label, entryIcon)

    super(provider, entry, {
      title: target.title,
      meta: joinParts([entry.repository, action.summary]),
      entryIcon,
      searchTerms: [target.title, entry.repository, action.summary],
      calendarDurationMinutes: null,
      changeStack,
      detailLink: target.webUrl ? { label: linkLabel, url: target.webUrl } : null,
      detailFields: [
        { label: 'Action', value: action.detail },
        { label: 'Repository', value: entry.repository },
        { label: itemLabel, value: number },
      ],
      badge,
      badges: [badge],
      detailSections: [],
      styleKey: provider.source,
    })
  }
}

class GitHubProviderAdapter extends ProviderUiAdapterDefinition<
  'github',
  GitHubEntry,
  GitHubEntryUiAdapter
> {
  readonly source = 'github' as const
  readonly label = 'GitHub'
  readonly providerIcon = 'code'

  create(entry: GitHubEntry): GitHubEntryUiAdapter {
    return new GitHubEntryUiAdapter(this, entry)
  }
}

export const githubAdapter = new GitHubProviderAdapter()
