import type { ActivityEntry, GitEntry, GitLabEntry } from '../generated/activity'

export function isMergedEntry(e: ActivityEntry): e is GitEntry & { gitlabEvent: GitLabEntry } {
  return e.source === 'git' && !!e.gitlabEvent
}

export function getEntryType(e: ActivityEntry): string | undefined {
  switch (e.source) {
    case 'gitlab':
    case 'jira':
    case 'system':
      return e.type
    default:
      return undefined
  }
}
