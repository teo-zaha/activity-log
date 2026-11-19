import type { MeetingWeight } from '../generated/activity'
import { formatDuration } from '../providers/utils'
import { addDaysToKey, dateOfKey } from './local-day'

/** The parts of an entry that meeting time reads; `EntryUiAdapter` has them. */
export interface MeetingLike {
  readonly timestamp: string
  readonly calendarDurationMinutes: number | null
  /** From the backend; null for entries that are no meetings. */
  readonly meetingWeight: MeetingWeight | null
}

/** Minutes of one local day (or several) covered by meetings, overlaps counted once. */
export interface MeetingTime {
  readonly busyMinutes: number
  /** Tentative time not already covered by busy meetings. */
  readonly tentativeMinutes: number
}

export const NO_MEETING_TIME: MeetingTime = { busyMinutes: 0, tentativeMinutes: 0 }

interface Interval {
  readonly start: number
  readonly end: number
}

/** Local midnight starting a `YYYY-MM-DD` day, in epoch milliseconds. */
function startOfDay(key: string): number {
  const date = dateOfKey(key)
  date.setHours(0, 0, 0, 0)
  return date.getTime()
}

/** The total length of the union of `intervals`. */
function unionLength(intervals: Interval[]): number {
  const sorted = [...intervals].sort((a, b) => a.start - b.start)
  let total = 0
  let current: Interval | null = null
  for (const interval of sorted) {
    if (current && interval.start <= current.end) {
      current = { start: current.start, end: Math.max(current.end, interval.end) }
    } else {
      if (current) total += current.end - current.start
      current = interval
    }
  }
  return current ? total + current.end - current.start : total
}

/**
 * Meeting time for each local day in `dayKeys`: the shown meetings with a duration, weighed as
 * the backend says and clipped to the day. A meeting across midnight counts on both days; a 23- or 25-hour day
 * (daylight saving) is clipped at its own midnights.
 */
export function meetingTimeByDay(
  entries: readonly MeetingLike[],
  dayKeys: readonly string[]
): Map<string, MeetingTime> {
  const meetings = entries.flatMap(entry => {
    const minutes = entry.calendarDurationMinutes
    const weight = entry.meetingWeight
    if (minutes == null || minutes <= 0 || weight === null || weight === 'none') return []
    const start = Date.parse(entry.timestamp)
    return [{ start, end: start + minutes * 60_000, weight }]
  })

  const result = new Map<string, MeetingTime>()
  for (const key of dayKeys) {
    const dayStart = startOfDay(key)
    const dayEnd = startOfDay(addDaysToKey(key, 1))
    const busy: Interval[] = []
    const all: Interval[] = []
    for (const meeting of meetings) {
      const start = Math.max(meeting.start, dayStart)
      const end = Math.min(meeting.end, dayEnd)
      if (end <= start) continue
      all.push({ start, end })
      if (meeting.weight === 'busy') busy.push({ start, end })
    }
    const busyMs = unionLength(busy)
    result.set(key, {
      busyMinutes: Math.round(busyMs / 60_000),
      tentativeMinutes: Math.round((unionLength(all) - busyMs) / 60_000),
    })
  }
  return result
}

export function sumMeetingTime(times: Iterable<MeetingTime>): MeetingTime {
  let busyMinutes = 0
  let tentativeMinutes = 0
  for (const time of times) {
    busyMinutes += time.busyMinutes
    tentativeMinutes += time.tentativeMinutes
  }
  return { busyMinutes, tentativeMinutes }
}

export interface MeetingTimeParts {
  /** `5h 30m`, null without busy time. */
  readonly busy: string | null
  /** `+1h`, or `1h` without busy time; the views add the word "tentative". */
  readonly tentative: string | null
}

/** The amounts a header shows, null without meetings. */
export function meetingTimeParts(time: MeetingTime): MeetingTimeParts | null {
  if (time.busyMinutes === 0 && time.tentativeMinutes === 0) return null
  return {
    busy: time.busyMinutes > 0 ? formatDuration(time.busyMinutes) : null,
    tentative:
      time.tentativeMinutes > 0
        ? `${time.busyMinutes > 0 ? '+' : ''}${formatDuration(time.tentativeMinutes)}`
        : null,
  }
}

/** `5h 30m · +1h tentative`, or null without meetings. */
export function meetingTimeLabel(time: MeetingTime): string | null {
  const parts = meetingTimeParts(time)
  if (!parts) return null
  const tentative = parts.tentative ? `${parts.tentative} tentative` : null
  return [parts.busy, tentative].filter(Boolean).join(' · ')
}
