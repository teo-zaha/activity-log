import { describe, expect, it } from 'vitest'
import type { LogEntry } from '../generated/activity'
import { formatLogTime, formatLogsAsText, matchesLevelFilter } from './log-format'

const entry = (overrides: Partial<LogEntry>): LogEntry => ({
  id: 1,
  timestamp: '2026-11-03T12:00:00Z',
  level: 'info',
  scope: 'git',
  message: 'Scanning 3 repositories',
  ...overrides,
})

describe('log-format', () => {
  it('filters by minimum level', () => {
    expect(matchesLevelFilter('info', 'all')).toBe(true)
    expect(matchesLevelFilter('info', 'warn')).toBe(false)
    expect(matchesLevelFilter('error', 'warn')).toBe(true)
    expect(matchesLevelFilter('warn', 'error')).toBe(false)
  })

  it('formats local wall-clock time and keeps unparsable values', () => {
    const local = new Date(2026, 10, 3, 9, 5, 7).toISOString()
    expect(formatLogTime(local)).toBe('09:05:07')
    expect(formatLogTime('not a date')).toBe('not a date')
  })

  it('renders aligned plain-text lines', () => {
    const text = formatLogsAsText([
      entry({}),
      entry({ id: 2, level: 'error', scope: 'outlook', message: 'Collection failed' }),
    ])
    expect(text).toBe(
      '2026-11-03T12:00:00Z INFO  [git] Scanning 3 repositories\n' +
        '2026-11-03T12:00:00Z ERROR [outlook] Collection failed'
    )
  })
})
