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
})
