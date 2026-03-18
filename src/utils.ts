import type { ActivityLog, ActivityEntry, GitEntry, GitLabEntry, JiraEntry, OutlookEntry, SystemEntry } from './types.js'

export function mergeGitWithGitlab(gitEntries: ActivityEntry[], gitlabEntries: ActivityEntry[]): {
  mergedGit: ActivityEntry[]
  remainingGitlab: ActivityEntry[]
} {
  const gitlabByCommit = new Map<string, GitLabEntry>()
  for (const e of gitlabEntries as GitLabEntry[]) {
    if (e.type !== 'pushed to') continue
    const commitTo = (e.raw.push_data as Record<string, unknown> | undefined)?.commit_to as string | undefined
    if (commitTo) gitlabByCommit.set(commitTo, e)
  }
  const mergedGit = (gitEntries as GitEntry[]).map(e => {
    const gitlabEvent = gitlabByCommit.get(e.hash)
    return gitlabEvent ? { ...e, gitlabEvent } : e
  })
  const remainingGitlab = (gitlabEntries as GitLabEntry[]).filter(e => {
    if (e.type !== 'pushed to') return true
    const commitTo = (e.raw.push_data as Record<string, unknown> | undefined)?.commit_to as string | undefined
    return !commitTo || !gitlabByCommit.has(commitTo)
  })
  return { mergedGit, remainingGitlab }
}

export function toMarkdown(log: ActivityLog): string {
  const { since, until } = log.meta
  let md = `# Activity Summary — ${since} to ${until}\n\nTotal: ${log.entries.length} events\n\n`

  const byDay = new Map<string, typeof log.entries>()
  for (const e of log.entries) {
    const day = e.timestamp?.slice(0, 10) ?? 'unknown'
    if (!byDay.has(day)) byDay.set(day, [])
    byDay.get(day)!.push(e)
  }
  const days = Array.from(byDay.keys()).sort((a, b) => b.localeCompare(a))

  for (const day of days) {
    const label = new Date(day + 'T12:00:00Z').toLocaleDateString('en-GB', {
      weekday: 'long', year: 'numeric', month: 'long', day: 'numeric',
    })
    md += `## ${label}\n\n`
    for (const e of byDay.get(day)!) {
      const t = new Date(e.timestamp).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
      if (e.source === 'git') {
        const g = e as GitEntry
        md += `- **[git]** \`${g.repo}\` ${g.message} (${t})\n`
      } else if (e.source === 'gitlab') {
        const g = e as GitLabEntry
        md += `- **[gitlab]** ${g.type}: ${g.title || ''} (${t})\n`
      } else if (e.source === 'jira') {
        const j = e as JiraEntry
        md += `- **[jira]** ${j.key}: ${j.title} [${j.status}] (${t})\n`
      } else if (e.source === 'outlook') {
        const o = e as OutlookEntry
        const dur = o.duration_minutes >= 60
          ? `${Math.floor(o.duration_minutes / 60)}h${o.duration_minutes % 60 ? ` ${o.duration_minutes % 60}m` : ''}`
          : `${o.duration_minutes}m`
        md += `- **[outlook]** ${o.title}${o.location ? ` @ ${o.location}` : ''} (${t}, ${dur})\n`
      } else if (e.source === 'system') {
        const s = e as SystemEntry
        md += `- **[system]** ${s.label} (${t})\n`
      }
    }
    md += '\n'
  }
  return md
}

export function today(): string {
  return new Date().toISOString().slice(0, 10)
}

export function daysAgo(n: number): string {
  const d = new Date()
  d.setDate(d.getDate() - n)
  return d.toISOString().slice(0, 10)
}

export function resolveRange(range: string): { since: string; until: string } {
  const now = new Date()
  const fmt = (d: Date): string => d.toISOString().slice(0, 10)
  switch (range) {
    case 'today':
      return { since: fmt(now), until: fmt(now) }
    case 'yesterday': {
      const y = new Date(now); y.setDate(y.getDate() - 1)
      return { since: fmt(y), until: fmt(y) }
    }
    case 'week': {
      const s = new Date(now); s.setDate(now.getDate() - now.getDay())
      return { since: fmt(s), until: fmt(now) }
    }
    case 'lastweek': {
      const e = new Date(now); e.setDate(now.getDate() - now.getDay() - 1)
      const s = new Date(e); s.setDate(e.getDate() - 6)
      return { since: fmt(s), until: fmt(e) }
    }
    case 'month':
      return { since: fmt(new Date(now.getFullYear(), now.getMonth(), 1)), until: fmt(now) }
    case 'lastmonth': {
      const s = new Date(now.getFullYear(), now.getMonth() - 1, 1)
      const e = new Date(now.getFullYear(), now.getMonth(), 0)
      return { since: fmt(s), until: fmt(e) }
    }
    default:
      throw new Error(`Unknown range: "${range}". Use today|yesterday|week|lastweek|month|lastmonth`)
  }
}
