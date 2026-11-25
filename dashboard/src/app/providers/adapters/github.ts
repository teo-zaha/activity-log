import type { GitHubEntry, IssueRef, PullRequestRef } from '../../generated/activity'
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

function issueDetails(issue: IssueRef) {
  return {
    target: issue,
    itemLabel: 'Issue',
    referencePrefix: CODE_HOST_REFERENCE_PREFIX.issue,
    linkLabel: 'Open issue',
    entryIcon: 'adjust',
    changeStack: null,
  }
}

function detailsOf(entry: Exclude<GitHubEntry, { type: 'committed' }>) {
  switch (entry.type) {
    case 'opened':
      return entry.target.kind === 'pullRequest'
        ? pullDetails(entry.target)
        : issueDetails(entry.target)
    case 'reviewed':
      return pullDetails(entry.pullRequest)
    case 'commentedOn':
      switch (entry.target.kind) {
        case 'pullRequest':
          return pullDetails(entry.target)
        case 'issue':
          return issueDetails(entry.target)
        case 'reviewLine':
          return pullDetails(entry.target.pullRequest)
      }
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
    const body = entry.type === 'commentedOn' ? entry.body : undefined
    const path =
      entry.type === 'commentedOn' && entry.target.kind === 'reviewLine'
        ? entry.target.path
        : undefined

    super(provider, entry, {
      title: target.title,
      meta: joinParts([entry.repository, action.summary]),
      entryIcon,
      searchTerms: toDefinedStrings([target.title, entry.repository, action.summary, body]),
      calendarDurationMinutes: null,
      changeStack,
      detailLink: target.webUrl ? { label: linkLabel, url: target.webUrl } : null,
      detailFields: [
        { label: 'Action', value: action.detail },
        { label: 'Repository', value: entry.repository },
        { label: itemLabel, value: number },
        ...(path ? [{ label: 'File', value: path }] : []),
        ...(body ? [{ label: 'Comment', value: body }] : []),
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
