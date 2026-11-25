import type {
  GitHubEntry,
  GitHubReviewState,
  GitLabEntry,
  GitLabType,
  Item,
} from '../generated/activity'
import { type ChangeReferencePrefix, changeReference } from './change-stack'

type CodeHostItem = 'mergeRequest' | Item['kind']

export const CODE_HOST_REFERENCE_PREFIX = {
  mergeRequest: '!',
  pullRequest: '#',
  issue: '#',
} as const satisfies Record<CodeHostItem, ChangeReferencePrefix>

const ITEM_WORDS: Record<CodeHostItem, string> = {
  mergeRequest: 'merge request',
  pullRequest: 'pull request',
  issue: 'issue',
}

const ACTION_WORDS: Record<GitLabType, string> = {
  pushedTo: 'Pushed',
  pushedNew: 'Pushed new branch',
  created: 'Created',
  opened: 'Opened',
  closed: 'Closed',
  reopened: 'Reopened',
  updated: 'Updated',
  merged: 'Merged',
  accepted: 'Accepted',
  commentedOn: 'Commented',
  approved: 'Approved',
  joined: 'Joined',
  destroyed: 'Deleted',
  expired: 'Expired',
  unknown: 'Activity',
}

const REVIEW_STATE_WORDS: Record<GitHubReviewState, string | undefined> = {
  approved: 'approved',
  changesRequested: 'changes requested',
  commented: undefined,
  dismissed: 'dismissed',
}

type GitLabTarget = { label: string; kind: 'note' | 'item' }

const GITLAB_TARGETS: ReadonlyMap<string, GitLabTarget> = new Map([
  ['Commit', { label: 'Commit', kind: 'item' }],
  ['Issue', { label: 'Issue', kind: 'item' }],
  ['MergeRequest', { label: 'Merge request', kind: 'item' }],
  ['Milestone', { label: 'Milestone', kind: 'item' }],
  ['Project', { label: 'Project', kind: 'item' }],
  ['Snippet', { label: 'Snippet', kind: 'item' }],
  ['WikiPage', { label: 'Wiki page', kind: 'item' }],
  ['WorkItem', { label: 'Work item', kind: 'item' }],
  ['Note', { label: 'Comment', kind: 'note' }],
  ['DiffNote', { label: 'Inline comment', kind: 'note' }],
  ['DiscussionNote', { label: 'Discussion comment', kind: 'note' }],
])

export function gitlabTargetLabel(targetType: string): string {
  return GITLAB_TARGETS.get(targetType)?.label ?? targetType
}

type ActionWords = { summary: string; detail: string }

function words(action: string): ActionWords {
  return { summary: action, detail: action }
}

function itemAction(
  action: string,
  kind: CodeHostItem,
  number: number,
  state?: string
): ActionWords {
  const subject = `${action} ${ITEM_WORDS[kind]}`
  const reference = changeReference(CODE_HOST_REFERENCE_PREFIX[kind], number)
  const suffix = state ? ` (${state})` : ''
  return { summary: `${subject} ${reference}${suffix}`, detail: subject + suffix }
}

function gitlabItemAction(action: string, entry: GitLabEntry): ActionWords {
  const verb = entry.type === 'commentedOn' || entry.type === 'unknown' ? `${action} on` : action
  if (entry.mrIid != null) return itemAction(verb, 'mergeRequest', entry.mrIid)
  if (entry.issueIid != null) return itemAction(verb, 'issue', entry.issueIid)
  if (!entry.targetType) return words(action)
  const target = GITLAB_TARGETS.get(entry.targetType)
  if (entry.type === 'commentedOn' && target?.kind === 'note') return words(action)
  return words(`${verb} ${target?.label.toLowerCase() ?? 'item'}`)
}

export function gitlabAction(entry: GitLabEntry): ActionWords {
  const action = ACTION_WORDS[entry.type]
  switch (entry.type) {
    case 'pushedTo':
      return words(entry.branch ? `${action} to ${entry.branch}` : action)
    case 'pushedNew':
      return words(entry.branch ? `${action} ${entry.branch}` : action)
    case 'approved':
    case 'created':
    case 'opened':
    case 'closed':
    case 'reopened':
    case 'updated':
    case 'merged':
    case 'accepted':
    case 'commentedOn':
    case 'joined':
    case 'destroyed':
    case 'expired':
    case 'unknown':
      return gitlabItemAction(action, entry)
  }
}

export function githubAction(entry: GitHubEntry): ActionWords {
  switch (entry.type) {
    case 'opened':
      return itemAction(ACTION_WORDS.opened, entry.target.kind, entry.target.number)
    case 'reviewed':
      return itemAction(
        'Reviewed',
        'pullRequest',
        entry.pullRequest.number,
        REVIEW_STATE_WORDS[entry.state]
      )
    case 'commentedOn':
      return entry.target.kind === 'reviewLine'
        ? itemAction(
            `Commented on ${entry.target.path} in`,
            'pullRequest',
            entry.target.pullRequest.number
          )
        : itemAction('Commented on', entry.target.kind, entry.target.number)
    case 'committed':
      return words(`Committed to ${entry.branch}`)
  }
}
