import { describe, expect, it } from 'vitest'
import type { GitHubReviewState } from '../../generated/activity'
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
