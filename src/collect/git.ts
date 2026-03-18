import { spawnSync } from 'child_process'
import { readdirSync, statSync, existsSync } from 'fs'
import { join, basename } from 'path'
import { homedir } from 'os'
import type { GitEntry, CollectOptions } from '../types.js'

function assertValidDate(d: string, name: string): void {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(d) || isNaN(Date.parse(d)))
    throw new Error(`[git] Invalid date for ${name}: ${JSON.stringify(d)}`)
}

function discoverRepos(root: string, depth = 0, max = 4): string[] {
  if (depth >= max) return []
  const repos: string[] = []
  try {
    for (const name of readdirSync(root)) {
      const full = join(root, name)
      try {
        if (!statSync(full).isDirectory()) continue
        if (existsSync(join(full, '.git'))) repos.push(full)
        else repos.push(...discoverRepos(full, depth + 1, max))
      } catch { /* skip */ }
    }
  } catch { /* skip */ }
  return repos
}

export async function collectGit(opts: CollectOptions): Promise<GitEntry[]> {
  assertValidDate(opts.since, 'since')
  assertValidDate(opts.until, 'until')

  const dirs = opts.gitDirs ?? discoverRepos(homedir())
  const entries: GitEntry[] = []
  for (const repoPath of dirs) {
    if (!existsSync(join(repoPath, '.git'))) continue
    const repoName = basename(repoPath)

    let authorEmail: string
    const emailResult = spawnSync('git', ['config', 'user.email'], { cwd: repoPath, encoding: 'utf8' })
    if (emailResult.error || emailResult.status !== 0) continue
    authorEmail = emailResult.stdout.trim()

    let log: string
    const logResult = spawnSync(
      'git',
      [
        'log', '--all',
        `--after=${opts.since}`,
        `--before=${opts.until}T23:59:59`,
        `--author=${authorEmail}`,
        '--format=%H|%aI|%ae|%s|%D',
      ],
      { cwd: repoPath, encoding: 'utf8' },
    )
    if (logResult.error || logResult.status !== 0) continue
    log = logResult.stdout
    for (const line of log.split('\n').filter(Boolean)) {
      const [hash, timestamp, author, message, refs] = line.split('|')
      const branch = refs?.split(',').map(r => r.trim()).find(r => r.startsWith('HEAD -> '))?.replace('HEAD -> ', '') ?? ''
      entries.push({ source: 'git', type: 'commit', repo: repoName, hash, timestamp, author, message, branch })
    }
  }
  return entries
}
