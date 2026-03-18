import type { GitLabEntry, CollectOptions } from '../types.js'
import { getCached, setCached } from '../cache.js'

function assertValidDate(d: string, name: string): void {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(d) || isNaN(Date.parse(d)))
    throw new Error(`[gitlab] Invalid date for ${name}: ${JSON.stringify(d)}`)
}

export async function collectGitLab(opts: CollectOptions): Promise<GitLabEntry[]> {
  assertValidDate(opts.since, 'since')
  assertValidDate(opts.until, 'until')

  const cacheKey = `gitlab-${opts.since}-${opts.until}`
  const cached = getCached<GitLabEntry[]>(cacheKey)
  if (cached) {
    console.log(`  [gitlab] cache hit (${cached.length} events)`)
    return cached
  }

  const { GITLAB_URL, GITLAB_TOKEN, GITLAB_USER } = process.env
  if (!GITLAB_TOKEN || !GITLAB_URL || !GITLAB_USER) {
    console.warn('  [gitlab] skipped — GITLAB_URL, GITLAB_TOKEN or GITLAB_USER not set')
    return []
  }
  const afterDate = new Date(opts.since)
  afterDate.setDate(afterDate.getDate() - 1)
  const afterParam = afterDate.toISOString().slice(0, 10)

  const beforeDateObj = new Date(opts.until)
  beforeDateObj.setDate(beforeDateObj.getDate() + 1)
  const beforeParam = beforeDateObj.toISOString().slice(0, 10)

  const entries: GitLabEntry[] = []
  let page = 1
  while (true) {
    const url = `${GITLAB_URL}/api/v4/users/${GITLAB_USER}/events?after=${afterParam}&before=${beforeParam}&per_page=100&page=${page}`
    const res = await fetch(url, { headers: { 'PRIVATE-TOKEN': GITLAB_TOKEN } })
    if (!res.ok) throw new Error(`GitLab API ${res.status}: ${await res.text()}`)
    const events = (await res.json()) as Record<string, unknown>[]
    if (!events.length) break
    for (const ev of events) {
      const pushData = ev.push_data as Record<string, unknown> | undefined
      const note = ev.note as Record<string, unknown> | undefined
      const notePosition = note?.position as Record<string, unknown> | undefined
      const targetType = (ev.target_type as string) ?? 'unknown'
      const isMR = targetType === 'MergeRequest'

      const entry: GitLabEntry = {
        source: 'gitlab',
        type: (ev.action_name as string) ?? 'unknown',
        timestamp: ev.created_at as string,
        title: (ev.target_title as string) ?? '',
        target_type: targetType,
        raw: ev,
      }
      if (isMR && ev.target_iid != null) entry.mr_iid = ev.target_iid as number
      if (pushData?.ref != null) entry.branch = pushData.ref as string
      if (pushData?.commit_count != null) entry.commit_count = pushData.commit_count as number
      if (note?.body != null) entry.note_body = note.body as string
      if (notePosition?.new_path != null) entry.note_file = notePosition.new_path as string
      if (note?.resolvable) entry.note_resolved = (note.resolved as boolean) ?? false
      entries.push(entry)
    }
    if (events.length < 100) break
    page++
  }
  setCached(cacheKey, entries)
  return entries
}
