import type { LogEntry, LogLevel } from '../generated/activity'

/** Minimum level shown in the logs pane. */
export type LogLevelFilter = 'all' | Exclude<LogLevel, 'info'>

const LEVEL_RANK: Record<LogLevel, number> = { info: 0, warn: 1, error: 2 }

export function matchesLevelFilter(level: LogLevel, filter: LogLevelFilter): boolean {
  return filter === 'all' || LEVEL_RANK[level] >= LEVEL_RANK[filter]
}

/** Local wall-clock time, e.g. `14:03:27`; falls back to the raw value if unparsable. */
export function formatLogTime(timestamp: string): string {
  const date = new Date(timestamp)
  if (Number.isNaN(date.getTime())) return timestamp
  const pad = (value: number): string => String(value).padStart(2, '0')
  return `${pad(date.getHours())}:${pad(date.getMinutes())}:${pad(date.getSeconds())}`
}

/** Plain-text log for bug reports, oldest entry first. */
export function formatLogsAsText(entries: readonly LogEntry[]): string {
  return entries
    .map(
      entry =>
        `${entry.timestamp} ${entry.level.toUpperCase().padEnd(5)} [${entry.scope}] ${entry.message}`
    )
    .join('\n')
}
