import 'dotenv/config'
import minimist from 'minimist'
import { writeFileSync, mkdirSync } from 'fs'
import { dirname } from 'path'
import { collectGit } from './collect/git.js'
import { collectGitLab } from './collect/gitlab.js'
import { collectJira } from './collect/jira.js'
import { collectOutlook } from './collect/outlook.js'
import { collectSystem } from './collect/system.js'
import type { ActivityLog, CollectOptions, GitEntry } from './types.js'
import { resolveRange, today, daysAgo, toMarkdown, mergeGitWithGitlab } from './utils.js'

const argv = minimist(process.argv.slice(2), {
  string: ['since', 'until', 'output', 'range', 'git-dirs', 'format'],
  default: { format: 'json' },
})

const DATE_RE = /^\d{4}-\d{2}-\d{2}$/

function assertValidDate(d: string, name: string): void {
  if (!DATE_RE.test(d) || isNaN(Date.parse(d)))
    throw new Error(`Invalid date for --${name}: ${JSON.stringify(d)} (expected YYYY-MM-DD)`)
}

type Format = 'json' | 'markdown'

function resolveFormat(): Format {
  const fmt = (argv.format as string).toLowerCase()
  if (fmt === 'json' || fmt === 'markdown') return fmt
  throw new Error(`Invalid --format "${fmt}". Use json or markdown.`)
}

function defaultOutput(format: Format): string {
  return format === 'markdown' ? 'logs/activity_log.md' : 'logs/activity_log.json'
}

function buildOptions(format: Format): CollectOptions {
  const output = (argv.output as string | undefined) ?? defaultOutput(format)
  if (argv.range) {
    const { since, until } = resolveRange(argv.range as string)
    return { since, until, output }
  }
  const since = (argv.since as string) ?? daysAgo(30)
  const until = (argv.until as string) ?? today()
  if (argv.since) assertValidDate(since, 'since')
  if (argv.until) assertValidDate(until, 'until')
  return {
    since,
    until,
    output,
    gitDirs: argv['git-dirs'] ? (argv['git-dirs'] as string).split(',') : undefined,
  }
}

async function main(): Promise<void> {
  const format = resolveFormat()
  const opts = buildOptions(format)
  console.log(`\n🔍 Collecting activity from ${opts.since} to ${opts.until} ...\n`)
  const [gitEntries, gitlabEntries, jiraEntries, outlookEntries, systemEntries] = await Promise.all([
    collectGit(opts).catch(e => { console.error('  [git] error:', e); return [] }),
    collectGitLab(opts).catch(e => { console.error('  [gitlab] error:', e); return [] }),
    collectJira(opts).catch(e => { console.error('  [jira] error:', e); return [] }),
    collectOutlook(opts).catch(e => { console.error('  [outlook] error:', e); return [] }),
    collectSystem(opts).catch(e => { console.error('  [system] error:', e); return [] }),
  ])
  const { mergedGit, remainingGitlab } = mergeGitWithGitlab(gitEntries, gitlabEntries)
  const mergedCount = mergedGit.filter(e => (e as GitEntry).gitlabEvent).length
  console.log(`  ✓ git     — ${mergedGit.length} commits (${mergedCount} merged with gitlab)`)
  console.log(`  ✓ gitlab  — ${remainingGitlab.length} events (${mergedCount} push events merged into git)`)
  console.log(`  ✓ jira    — ${jiraEntries.length} issues`)
  console.log(`  ✓ outlook — ${outlookEntries.length} calendar events`)
  console.log(`  ✓ system  — ${systemEntries.length} power events`)
  const log: ActivityLog = {
    meta: { generated: new Date().toISOString(), since: opts.since, until: opts.until },
    entries: [...mergedGit, ...remainingGitlab, ...jiraEntries, ...outlookEntries, ...systemEntries].sort(
      (a, b) => new Date(b.timestamp).getTime() - new Date(a.timestamp).getTime()
    ),
  }
  const content = format === 'markdown' ? toMarkdown(log) : JSON.stringify(log, null, 2)
  mkdirSync(dirname(opts.output), { recursive: true })
  writeFileSync(opts.output, content)
  console.log(`\n✅ ${log.entries.length} total entries → ${opts.output}\n`)
}

main().catch(err => { console.error('Fatal:', err); process.exit(1) })
