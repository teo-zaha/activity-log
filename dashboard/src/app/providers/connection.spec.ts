import { describe, expect, it } from 'vitest'
import type { GitEntry } from '../generated/activity'
import { connectionKey, entryConnectionKeys, parseConnectionKey, parseEntryKey } from './connection'

const commit: GitEntry = {
  source: 'git',
  connection: 'wsl-nixos',
  id: 'abc123',
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

  it('parses a key back only for a known kind and a name', () => {
    expect(parseConnectionKey('gitlab/oss')).toBe('gitlab/oss')
    expect(parseConnectionKey('git/wsl/nixos')).toBe('git/wsl/nixos')
    expect(parseConnectionKey('teams/me')).toBeNull()
    expect(parseConnectionKey('jira/')).toBeNull()
    expect(parseConnectionKey('jira')).toBeNull()
  })

  it('parses an entry key: a known kind, a connection name and an id that may hold slashes', () => {
    expect(parseEntryKey('git/wsl-nixos/1f3a')).toBe('git/wsl-nixos/1f3a')
    expect(parseEntryKey('calendar/team/uid/1@x.org')).toBe('calendar/team/uid/1@x.org')
    expect(parseEntryKey('teams/me/1')).toBeNull()
    expect(parseEntryKey('git/work')).toBeNull()
    expect(parseEntryKey('git/work/')).toBeNull()
    expect(parseEntryKey('git//1')).toBeNull()
  })

  it('names the merged GitLab push of a commit too', () => {
    expect(entryConnectionKeys(commit)).toEqual(['git/wsl-nixos'])
    expect(
      entryConnectionKeys({
        ...commit,
        gitlabEvent: {
          source: 'gitlab',
          connection: 'work',
          id: '4711',
          type: 'pushedTo',
          timestamp: commit.timestamp,
          title: 'Pushed',
          targetType: 'push',
          pushedCommit: commit.hash,
        },
      })
    ).toEqual(['git/wsl-nixos', 'gitlab/work'])
  })
})
