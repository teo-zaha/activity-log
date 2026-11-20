import type { ActivityEntry, ConnectionId, Source } from '../generated/activity'

/**
 * A connection as one comparable value, for sets and selections: `gitlab/oss`. Built from a typed
 * `ConnectionId` or entry; only a URL is parsed back, see `parseConnectionKey`.
 */
export type ConnectionKey = `${Source}/${string}`

export function connectionKey(id: ConnectionId): ConnectionKey {
  return `${id.source}/${id.name}`
}

const SOURCES: Readonly<Record<Source, true>> = {
  git: true,
  gitlab: true,
  jira: true,
  outlook: true,
  system: true,
  calendar: true,
}

/** Whether `value` names a source kind this dashboard has an adapter for. */
export function isSource(value: string): value is Source {
  return Object.hasOwn(SOURCES, value)
}

/** The key `value` spells, such as a URL's `gitlab/oss`, if it names a known kind and a name. */
export function parseConnectionKey(value: string): ConnectionKey | null {
  const slash = value.indexOf('/')
  const source = value.slice(0, slash)
  const name = value.slice(slash + 1)
  return slash > 0 && name && isSource(source) ? `${source}/${name}` : null
}

/** An entry as one comparable value: `outlook/work/AAMk…`, the `trackKey` of its adapter. */
export type EntryKey = `${Source}/${string}/${string}`

/**
 * The entry key `value` spells, such as a URL's `git/work/1f3a…`: a known kind, a connection name
 * and an id. Connection names never hold a `/`; ids may.
 */
export function parseEntryKey(value: string): EntryKey | null {
  const first = value.indexOf('/')
  const second = value.indexOf('/', first + 1)
  if (first <= 0 || second <= first + 1 || second === value.length - 1) return null
  const source = value.slice(0, first)
  const name = value.slice(first + 1, second)
  return isSource(source) ? `${source}/${name}/${value.slice(second + 1)}` : null
}

/** The connections an entry came from: a git commit also names its merged GitLab push. */
export function entryConnectionKeys(entry: ActivityEntry): ConnectionKey[] {
  const own: ConnectionKey = `${entry.source}/${entry.connection}`
  if (entry.source === 'git' && entry.gitlabEvent) {
    return [own, `gitlab/${entry.gitlabEvent.connection}`]
  }
  return [own]
}
