import { describe, expect, it } from 'vitest'
import type { CommentTarget, GitHubEntry, GitHubReviewState } from '../../generated/activity'
import { githubPullRequestEntry } from '../../testing/activity-fixtures'
import { createEntryAdapter } from '../provider-adapter'

describe('GitHub adapter', () => {
  it('names the subject of opened pull requests and issues', () => {
    const pull = createEntryAdapter(githubPullRequestEntry)
    const issue = createEntryAdapter({
      ...githubPullRequestEntry,
      target: { kind: 'issue', number: 7, title: 'Report a problem' },
    })

    expect(pull.meta).toBe('acme/widgets · Opened pull request #12')
    expect(issue.meta).toBe('acme/widgets · Opened issue #7')
    expect(issue.detailFields).toContainEqual({ label: 'Action', value: 'Opened issue' })
    expect(issue.detailFields).toContainEqual({ label: 'Issue', value: '#7' })
  })

  const stateChanges: [GitHubEntry, string, string][] = [
    [
      { ...githubPullRequestEntry, type: 'merged', pullRequest: githubPullRequestEntry.target },
      'Merged pull request #12',
      'Merged pull request',
    ],
    [
      { ...githubPullRequestEntry, type: 'closed' },
      'Closed pull request #12',
      'Closed pull request',
    ],
    [
      {
        ...githubPullRequestEntry,
        type: 'closed',
        target: { kind: 'issue', number: 5, title: 'Fix widgets' },
      },
      'Closed issue #5',
      'Closed issue',
    ],
  ]

  it.each(stateChanges)(
    'words a merge or close with its typed subject',
    (event, summary, action) => {
      const entry = createEntryAdapter(event)
      expect(entry.meta).toBe(`acme/widgets · ${summary}`)
      expect(entry.detailFields).toContainEqual({ label: 'Action', value: action })
    }
  )

  const reviewWords: [GitHubReviewState, string][] = [
    ['approved', ' (approved)'],
    ['changesRequested', ' (changes requested)'],
    ['commented', ''],
    ['dismissed', ' (dismissed)'],
  ]

  it.each(reviewWords)('words a %s review', (state, suffix) => {
    const entry = createEntryAdapter({
      ...githubPullRequestEntry,
      type: 'reviewed',
      pullRequest: githubPullRequestEntry.target,
      state,
    })

    expect(entry.meta).toBe(`acme/widgets · Reviewed pull request #12${suffix}`)
    expect(entry.detailFields).toContainEqual({
      label: 'Action',
      value: `Reviewed pull request${suffix}`,
    })
  })

  const conversationWords: [CommentTarget, string][] = [
    [githubPullRequestEntry.target, 'Commented on pull request #12'],
    [{ kind: 'issue', number: 5, title: 'Fix widgets' }, 'Commented on issue #5'],
  ]

  it.each(conversationWords)(
    'names the item a conversation comment belongs to',
    (target, action) => {
      const entry = createEntryAdapter({
        ...githubPullRequestEntry,
        type: 'commentedOn',
        target,
      })

      expect(entry.meta).toBe(`acme/widgets · ${action}`)
      expect(entry.detailFields.some(({ label }) => label === 'Comment')).toBe(false)
    }
  )

  it('shows the file, body, and pull request link of an inline comment', () => {
    const webUrl = 'https://example.org/acme/widgets/pull/12'
    const entry = createEntryAdapter({
      ...githubPullRequestEntry,
      type: 'commentedOn',
      target: {
        kind: 'reviewLine',
        pullRequest: { ...githubPullRequestEntry.target, webUrl },
        path: 'src/main.rs',
      },
      body: 'Could we add a test?',
    })

    expect(entry.meta).toBe('acme/widgets · Commented on src/main.rs in pull request #12')
    expect(entry.detailFields).toContainEqual({ label: 'File', value: 'src/main.rs' })
    expect(entry.detailFields).toContainEqual({ label: 'Comment', value: 'Could we add a test?' })
    expect(entry.detailLink).toEqual({ label: 'Open pull request', url: webUrl })
  })

  it('keeps the branch name in a commit action', () => {
    const entry = createEntryAdapter({
      ...githubPullRequestEntry,
      type: 'committed',
      sha: 'abcdef012345',
      branch: 'main',
    })

    expect(entry.meta).toBe('acme/widgets · Committed to main · abcdef0')
  })
})
