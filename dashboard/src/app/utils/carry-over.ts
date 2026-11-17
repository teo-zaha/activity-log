import type { ActivityEntry, ActivityLog } from '../generated/activity'
import { type ConnectionKey, connectionKey } from '../providers/connection'

type GitLabEntry = Extract<ActivityEntry, { source: 'gitlab' }>

/**
 * The entries to show for `log`. While it is partial, the connections it still waits for keep
 * their `previous` entries inside the new range, so collecting again does not blank them out.
 *
 * The same change must not show twice meanwhile, so what the backend does once everything is in
 * is done here for the carried entries:
 * - a carried Git commit is dropped when the log already has its hash: two connections can hold
 *   the same commit, and until both are in the backend lists it under the one that finished;
 * - a GitLab push in the log is folded into the carried Git commit it pushed.
 */
export function withCarriedEntries(
  log: ActivityLog,
  previous: readonly ActivityEntry[]
): ActivityEntry[] {
  const pending = new Set<ConnectionKey>((log.meta.pending ?? []).map(connectionKey))
  if (pending.size === 0 || previous.length === 0) return log.entries

  const start = Date.parse(log.meta.start)
  const end = Date.parse(log.meta.end)
  const freshCommits = new Set(
    log.entries.flatMap(entry => (entry.source === 'git' ? [entry.hash] : []))
  )
  const carried = previous.filter(entry => {
    if (!pending.has(`${entry.source}/${entry.connection}`)) return false
    const at = Date.parse(entry.timestamp)
    if (!(start <= at && at < end)) return false
    return !(entry.source === 'git' && freshCommits.has(entry.hash))
  })
  if (carried.length === 0) return log.entries

  const pushes = new Map<string, GitLabEntry>()
  for (const entry of log.entries) {
    const commit = pushedCommit(entry)
    if (commit !== null && entry.source === 'gitlab') pushes.set(commit, entry)
  }
  const folded = new Set<GitLabEntry>()
  const carriedWithPushes = carried.map(entry => {
    const push = entry.source === 'git' ? pushes.get(entry.hash) : undefined
    if (entry.source !== 'git' || !push) return entry
    folded.add(push)
    return { ...entry, gitlabEvent: push }
  })
  const fresh = log.entries.filter(entry => entry.source !== 'gitlab' || !folded.has(entry))

  // Newest first, as the backend sorts a log.
  return [...fresh, ...carriedWithPushes].sort(
    (a, b) => Date.parse(b.timestamp) - Date.parse(a.timestamp)
  )
}

/** The commit a GitLab push event pushed, as the backend reads it from the event. */
function pushedCommit(entry: ActivityEntry): string | null {
  if (entry.source !== 'gitlab' || entry.type !== 'pushedTo') return null
  const pushData = entry.raw['push_data']
  if (typeof pushData !== 'object' || pushData === null || !('commit_to' in pushData)) return null
  return typeof pushData.commit_to === 'string' ? pushData.commit_to : null
}
