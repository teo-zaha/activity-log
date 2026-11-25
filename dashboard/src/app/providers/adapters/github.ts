import type { GitHubEntry, GitHubReviewState, PullRequestRef } from '../../generated/activity'
import { changeReference, changeStackVmOf } from '../change-stack'
import { EntryUiAdapter, ProviderUiAdapterDefinition } from '../types'
import { createBadge, joinParts, toDefinedStrings } from '../utils'

const REVIEW_ACTION: Record<GitHubReviewState, string> = {
  approved: 'Approved',
  changesRequested: 'Requested changes',
  commented: 'Reviewed',
  dismissed: 'Reviewed (dismissed)',
}

function pullDetails(pull: PullRequestRef, action: string) {
  return {
    target: pull,
    action,
    itemLabel: 'PR',
    linkLabel: 'Open pull request',
    entryIcon: 'code',
    changeStack: pull.changeStack ? changeStackVmOf(pull.changeStack, '#') : null,
  }
}

function detailsOf(entry: Exclude<GitHubEntry, { type: 'committed' }>) {
  switch (entry.type) {
    case 'opened':
      return entry.target.kind === 'pullRequest'
        ? pullDetails(entry.target, 'Opened')
        : {
            target: entry.target,
            action: 'Opened',
            itemLabel: 'Issue',
            linkLabel: 'Open issue',
            entryIcon: 'adjust',
            changeStack: null,
          }
    case 'reviewed':
      return pullDetails(entry.pullRequest, REVIEW_ACTION[entry.state])
  }
}

class GitHubEntryUiAdapter extends EntryUiAdapter<GitHubEntry> {
  constructor(provider: GitHubProviderAdapter, entry: GitHubEntry) {
    if (entry.type === 'committed') {
      const action = `Committed to ${entry.branch}`
      const badge = createBadge(provider.source, provider.label, 'commit')
      super(provider, entry, {
        title: entry.message ?? 'Commit',
        meta: joinParts([entry.repository, action, entry.sha.slice(0, 7)]),
        entryIcon: 'commit',
        searchTerms: toDefinedStrings([entry.message, entry.repository, action, entry.sha]),
        calendarDurationMinutes: null,
        detailLink: entry.webUrl ? { label: 'Open commit', url: entry.webUrl } : null,
        detailFields: [
          { label: 'Action', value: action },
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
    const { target, action, itemLabel, linkLabel, entryIcon, changeStack } = detailsOf(entry)
    const number = changeReference('#', target.number)
    const badge = createBadge(provider.source, provider.label, entryIcon)

    super(provider, entry, {
      title: target.title,
      meta: joinParts([entry.repository, action, number]),
      entryIcon,
      searchTerms: [target.title, entry.repository, action, number],
      calendarDurationMinutes: null,
      changeStack,
      detailLink: target.webUrl ? { label: linkLabel, url: target.webUrl } : null,
      detailFields: [
        { label: 'Action', value: action },
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
