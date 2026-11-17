import type { ActivityEntry, ConnectionId, Source } from '../generated/activity'

/**
 * A connection as one comparable value, for sets and selections: `gitlab/oss`. Built only from
 * a typed `ConnectionId` or entry, never parsed back.
 */
export type ConnectionKey = `${Source}/${string}`

export function connectionKey(id: ConnectionId): ConnectionKey {
  return `${id.source}/${id.name}`
}

/** The connections an entry came from: a git commit also names its merged GitLab push. */
export function entryConnectionKeys(entry: ActivityEntry): ConnectionKey[] {
  const own: ConnectionKey = `${entry.source}/${entry.connection}`
  if (entry.source === 'git' && entry.gitlabEvent) {
    return [own, `gitlab/${entry.gitlabEvent.connection}`]
  }
  return [own]
}
