import { describe, expect, it } from 'vitest'
import type { GitEntry } from '../generated/activity'
import { connectionKey, entryConnectionKeys } from './connection'

const commit: GitEntry = {
  source: 'git',
  connection: 'wsl-nixos',
  timestamp: '2026-05-18T09:00:00Z',
  repo: 'api',
  hash: 'abc',
  message: 'Fix',
  author: 'me@example.com',
  branch: 'main',
}

describe('connection keys', () => {
  it('joins kind and name', () => {
    expect(connectionKey({ source: 'gitlab', name: 'oss' })).toBe('gitlab/oss')
  })

  it('names the merged GitLab push of a commit too', () => {
    expect(entryConnectionKeys(commit)).toEqual(['git/wsl-nixos'])
    expect(
      entryConnectionKeys({
        ...commit,
        gitlabEvent: {
          source: 'gitlab',
          connection: 'work',
          type: 'pushedTo',
          timestamp: commit.timestamp,
          title: 'Pushed',
          targetType: 'push',
          raw: {},
        },
      })
    ).toEqual(['git/wsl-nixos', 'gitlab/work'])
  })
})
