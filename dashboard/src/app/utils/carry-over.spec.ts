import { describe, expect, it } from 'vitest'
import type { ActivityEntry, ActivityLog, ConnectionId } from '../generated/activity'
import { withCarriedEntries } from './carry-over'

type GitEntry = Extract<ActivityEntry, { source: 'git' }>

function commit(connection: string, hash: string, timestamp: string): GitEntry {
  return {
    source: 'git',
    connection,
    id: hash,
    timestamp,
    repo: 'activity-log',
    hash,
    message: hash,
    author: 'Dev',
    branch: 'main',
  }
}

/** A log of 17–18 May (UTC) with `entries`, still waiting for `pending`. */
function logOf(entries: ActivityEntry[], pending: ConnectionId[]): ActivityLog {
  return {
    meta: {
      generated: '2026-05-19T08:00:00Z',
      start: '2026-05-17T00:00:00Z',
      end: '2026-05-19T00:00:00Z',
      ...(pending.length ? { pending } : {}),
    },
    summary: { connections: [], mergedCount: 0 },
    entries,
  }
}

const laptop: ConnectionId = { source: 'git', name: 'laptop' }
const hashes = (entries: ActivityEntry[]): string[] =>
  entries.map(entry => (entry.source === 'git' ? entry.hash : entry.source))

describe('withCarriedEntries', () => {
  it('keeps the previous entries of pending connections, newest first', () => {
    const fresh = commit('server', 'fresh', '2026-05-18T09:00:00Z')
    const previous = [
      commit('laptop', 'newer', '2026-05-18T10:00:00Z'),
      commit('laptop', 'older', '2026-05-17T10:00:00Z'),
    ]

    expect(hashes(withCarriedEntries(logOf([fresh], [laptop]), previous))).toEqual([
      'newer',
      'fresh',
      'older',
    ])
  })

  it('drops previous entries of connections that are in, or outside the new range', () => {
    const previous = [
      commit('server', 'replaced', '2026-05-18T10:00:00Z'),
      commit('laptop', 'before', '2026-05-16T23:59:59Z'),
      commit('laptop', 'at-end', '2026-05-19T00:00:00Z'),
      commit('laptop', 'inside', '2026-05-17T00:00:00Z'),
    ]

    expect(hashes(withCarriedEntries(logOf([], [laptop]), previous))).toEqual(['inside'])
  })

  it('shows a commit once when another connection already delivered it', () => {
    const fresh = commit('server', 'shared', '2026-05-18T09:00:00Z')
    const previous = [
      commit('laptop', 'shared', '2026-05-18T09:00:00Z'),
      commit('laptop', 'own', '2026-05-18T08:00:00Z'),
    ]

    const shown = withCarriedEntries(logOf([fresh], [laptop]), previous)

    expect(hashes(shown)).toEqual(['shared', 'own'])
    expect(shown[0]).toBe(fresh)
  })

  it('carries nothing into a finished log', () => {
    const log = logOf([commit('server', 'fresh', '2026-05-18T09:00:00Z')], [])

    expect(withCarriedEntries(log, [commit('laptop', 'old', '2026-05-18T10:00:00Z')])).toBe(
      log.entries
    )
  })

  it('folds a push that is in already into the carried commit it pushed', () => {
    const push = (commitTo: string): ActivityEntry => ({
      source: 'gitlab',
      connection: 'work',
      id: commitTo,
      type: 'pushedTo',
      timestamp: '2026-05-18T09:05:00Z',
      title: 'activity-log',
      targetType: 'Project',
      pushedCommit: commitTo,
    })
    const [pushed, unmatched] = [push('carried'), push('elsewhere')]
    const previous = [commit('laptop', 'carried', '2026-05-18T09:00:00Z')]

    const shown = withCarriedEntries(logOf([pushed, unmatched], [laptop]), previous)

    expect(shown).toEqual([unmatched, { ...previous[0], gitlabEvent: pushed }])
  })
})
