import { describe, expect, it } from 'vitest'
import type { GitLabEntry, GitLabType } from '../../generated/activity'
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
  it('words a note action using the underlying merge request or issue', () => {
    const mergeRequest = createEntryAdapter({
      ...opened,
      type: 'commentedOn',
      targetType: 'DiffNote',
      issueIid: undefined,
      mrIid: 12,
    })
    const issue = createEntryAdapter({ ...opened, type: 'commentedOn', targetType: 'Note' })

    expect(mergeRequest.meta).toBe('Commented on merge request !12')
    expect(mergeRequest.detailFields).toContainEqual({
      label: 'Action',
      value: 'Commented on merge request',
    })
    expect(mergeRequest.detailFields).toContainEqual({
      label: 'Target type',
      value: 'Inline comment',
    })
    expect(mergeRequest.searchText).toContain('commented on merge request !12')
    expect(issue.meta).toBe('Commented on issue #53')
  })

  const pushWords: [GitLabType, string | undefined, string][] = [
    ['pushedTo', 'main', 'Pushed to main'],
    ['pushedTo', undefined, 'Pushed'],
    ['pushedNew', 'feature-x', 'Pushed new branch feature-x'],
    ['pushedNew', undefined, 'Pushed new branch'],
  ]

  it.each(pushWords)('words a %s push to %s', (type, branch, title) => {
    const push = createEntryAdapter({
      ...opened,
      type,
      title: undefined,
      targetType: undefined,
      issueIid: undefined,
      branch,
    })

    expect(push.title).toBe(title)
    expect(push.meta).toBe('')
    expect(push.detailFields).toContainEqual({ label: 'Action', value: title })
  })

  it('uses generic words when the target is unknown', () => {
    const unknown = createEntryAdapter({
      ...opened,
      type: 'updated',
      targetType: 'FutureTarget',
      issueIid: undefined,
    })
    const note = createEntryAdapter({
      ...opened,
      type: 'commentedOn',
      targetType: 'DiffNote',
      issueIid: undefined,
    })

    expect(unknown.meta).toBe('Updated item')
    expect(unknown.detailFields).toContainEqual({ label: 'Target type', value: 'FutureTarget' })
    expect(note.meta).toBe('Commented')
  })

  it('uses neutral wording for an unrecognized action', () => {
    const entry = createEntryAdapter({ ...opened, type: 'unknown' })

    expect(entry.meta).toBe('Activity on issue #53')
    expect(entry.detailFields).toContainEqual({ label: 'Action', value: 'Activity on issue' })
  })

  it('names an issue as an issue, not as a merge request', () => {
    const fields = createEntryAdapter(opened).detailFields

    expect(fields).toContainEqual({ label: 'Issue', value: '#53' })
    expect(fields.some(field => field.label === 'MR')).toBe(false)
  })

  it.each([
    ['opened', 'Opened merge request'],
    ['merged', 'Merged merge request'],
    ['approved', 'Approved merge request'],
  ] as const)('words a %s merge request', (type, action) => {
    const entry = createEntryAdapter({
      ...opened,
      type,
      targetType: 'MergeRequest',
      issueIid: undefined,
      mrIid: 12,
    })

    expect(entry.meta).toBe(`${action} !12`)
    expect(entry.detailFields).toContainEqual({ label: 'Action', value: action })
    expect(entry.detailFields).toContainEqual({ label: 'MR', value: '!12' })
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
