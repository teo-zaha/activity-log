import { type ActivityEntry, type ConnectionId, SOURCES, type Source } from '../generated/activity'

/**
 * A connection as one comparable value, for sets and selections: `gitlab/oss`. Built from a typed
 * `ConnectionId` or entry; only a URL is parsed back, see `parseConnectionKey`.
 */
export type ConnectionKey = `${Source}/${string}`

export function connectionKey(id: ConnectionId): ConnectionKey {
  return `${id.source}/${id.name}`
}

const KNOWN_SOURCES: ReadonlySet<string> = new Set(SOURCES)

/** Whether `value` names a source kind the backend has, each of which has an adapter here. */
export function isSource(value: string): value is Source {
  return KNOWN_SOURCES.has(value)
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

/** The entry's own connection followed by the connections that supplied any folded events. */
export function entryConnectionKeys(entry: ActivityEntry): ConnectionKey[] {
  const keys: ConnectionKey[] = [`${entry.source}/${entry.connection}`]
  if (entry.source === 'git') {
    if (entry.gitlabEvent) keys.push(`gitlab/${entry.gitlabEvent.connection}`)
    if (entry.githubEvent) keys.push(`github/${entry.githubEvent.connection}`)
  }
  return keys
}
