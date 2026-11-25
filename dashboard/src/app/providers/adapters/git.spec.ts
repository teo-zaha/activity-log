import { describe, expect, it } from 'vitest'
import type { GitEntry } from '../../generated/activity'
import { githubAdapter } from './github'
import { gitAdapter } from './git'

const sha = '0123456789abcdef0123456789abcdef01234567'
const commitUrl = `https://github.com/acme/widgets/commit/${sha}`
const commit = {
  source: 'git',
  connection: 'local',
  id: sha,
  timestamp: '2026-11-17T10:10:00Z',
  repo: 'widgets',
  hash: sha,
  message: 'Add widget',
  author: 'Dev',
  githubEvent: {
    source: 'github',
    connection: 'oss',
    id: 'C_example',
    timestamp: '2026-11-17T10:10:00Z',
    repository: 'acme/widgets',
    type: 'committed',
    sha,
    message: 'Add widget',
    branch: 'main',
    webUrl: commitUrl,
  },
} satisfies GitEntry

describe('code host commit activity', () => {
  it('shows the default branch and commit link for a standalone GitHub commit', () => {
    const entry = githubAdapter.create(commit.githubEvent)

    expect(entry.title).toBe('Add widget')
    expect(entry.meta).toContain('Committed to main')
    expect(entry.detailLink).toEqual({ label: 'Open commit', url: commitUrl })
  })

  it('keeps both hosts on the same Git row and under their connection filters', () => {
    const entry = gitAdapter.create({
      ...commit,
      gitlabEvent: {
        source: 'gitlab',
        connection: 'work',
        id: '77',
        timestamp: '2026-11-17T10:12:00Z',
        type: 'pushedTo',
        pushedCommit: sha,
      },
    })

    expect(entry.badge.splits).toEqual([
      { source: 'gitlab', label: 'Lab' },
      { source: 'github', label: 'Hub' },
    ])
    expect(entry.styleKey).toBe('git gitlab github')
    expect(entry.connections).toEqual(['git/local', 'gitlab/work', 'github/oss'])
    expect(entry.detailSections.map(section => section.source)).toEqual(['git', 'gitlab', 'github'])
    expect(entry.detailLink?.url).toBe(commitUrl)
  })
})
