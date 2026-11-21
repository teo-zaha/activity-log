import { describe, expect, it } from 'vitest'
import type { GitLabEntry } from '../../generated/activity'
import { createEntryAdapter } from '../provider-adapter'

const opened: GitLabEntry = {
  source: 'gitlab',
  connection: 'work',
  id: '1',
  type: 'opened',
  timestamp: '2026-11-17T09:00:00Z',
  title: 'Broken build',
  targetType: 'Issue',
  issueIid: 53,
}

describe('GitLab adapter', () => {
  it('names an issue as an issue, not as a merge request', () => {
    const fields = createEntryAdapter(opened).detailFields

    expect(fields).toContainEqual({ label: 'Issue', value: '#53' })
    expect(fields.some(field => field.label === 'MR')).toBe(false)
  })

  it('names a merge request by its iid', () => {
    const fields = createEntryAdapter({
      ...opened,
      targetType: 'MergeRequest',
      issueIid: undefined,
      mrIid: 12,
    }).detailFields

    expect(fields).toContainEqual({ label: 'MR', value: '!12' })
  })

  it('lays out a merge request’s stack with GitLab’s `!`, and none without one', () => {
    const mergeRequest: GitLabEntry = {
      ...opened,
      targetType: 'MergeRequest',
      issueIid: undefined,
      mrIid: 12,
    }
    const stacked = createEntryAdapter({
      ...mergeRequest,
      changeStack: {
        current: { number: 12, title: 'Broken build' },
        below: [{ number: 11, title: 'Groundwork' }],
        bottom: { kind: 'reached', branch: 'main' },
        above: { changes: [], end: { kind: 'complete' } },
      },
    })

    expect(stacked.changeStack?.chain.map(row => row.reference)).toEqual(['!11', '!12'])
    expect(createEntryAdapter(mergeRequest).changeStack).toBeNull()
  })
})
