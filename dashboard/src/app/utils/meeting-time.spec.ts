import { describe, expect, it } from 'vitest'
import { type DayKey, localDateKey } from './local-day'
import type { MeetingWeight } from '../generated/activity'
import {
  type MeetingLike,
  meetingTimeByDay,
  meetingTimeLabel,
  meetingTimeParts,
  sumMeetingTime,
} from './meeting-time'

// Local `Date` constructors, so every expectation holds in any time zone.
function meeting(
  day: number,
  hour: number,
  minute: number,
  durationMinutes: number,
  meetingWeight: MeetingWeight | null = 'busy'
): MeetingLike {
  return {
    timestamp: new Date(2026, 10, day, hour, minute).toISOString(),
    calendarDurationMinutes: durationMinutes,
    meetingWeight,
  }
}

function on(day: DayKey, entries: MeetingLike[]): { busy: number; tentative: number } {
  const time = meetingTimeByDay(entries, [day]).get(day)!
  return { busy: time.busyMinutes, tentative: time.tentativeMinutes }
}

describe('meetingTimeByDay', () => {
  it('adds separate meetings', () => {
    expect(on('2026-11-16', [meeting(16, 9, 0, 30), meeting(16, 14, 0, 60)])).toEqual({
      busy: 90,
      tentative: 0,
    })
  })

  it('counts overlapping meetings once', () => {
    expect(
      on('2026-11-16', [
        meeting(16, 9, 0, 60),
        meeting(16, 9, 30, 60),
        meeting(16, 9, 45, 15),
        // Touching, not overlapping.
        meeting(16, 10, 30, 30),
      ])
    ).toEqual({ busy: 120, tentative: 0 })
  })

  it('counts what the backend weighs as busy and leaves out what it weighs as none', () => {
    expect(
      on('2026-11-16', [
        meeting(16, 8, 0, 60, 'busy'),
        meeting(16, 10, 0, 60, 'busy'),
        meeting(16, 14, 0, 60, 'none'),
      ])
    ).toEqual({ busy: 120, tentative: 0 })
  })

  it('shows tentative time apart, without what busy meetings already cover', () => {
    expect(
      on('2026-11-16', [
        meeting(16, 9, 0, 60, 'busy'),
        // 09:30–11:00: half an hour under the busy meeting, an hour beyond it.
        meeting(16, 9, 30, 90, 'tentative'),
        // Two tentative meetings that overlap each other count once.
        meeting(16, 14, 0, 60, 'tentative'),
        meeting(16, 14, 30, 60, 'tentative'),
        // Fully inside a busy meeting: adds nothing.
        meeting(16, 9, 10, 20, 'tentative'),
      ])
    ).toEqual({ busy: 60, tentative: 150 })
  })

  it('splits a meeting across midnight between both days', () => {
    const late = [meeting(16, 23, 0, 120)]
    const days = meetingTimeByDay(late, ['2026-11-15', '2026-11-16', '2026-11-17', '2026-11-18'])

    expect(days.get('2026-11-15')!.busyMinutes).toBe(0)
    expect(days.get('2026-11-16')!.busyMinutes).toBe(60)
    expect(days.get('2026-11-17')!.busyMinutes).toBe(60)
    expect(days.get('2026-11-18')!.busyMinutes).toBe(0)
  })

  it('clips to the local day, however long daylight saving makes it', () => {
    // The last Sunday of March and of October are 23 and 25 hours long in much of Europe.
    for (const [month, day] of [
      [2, 29],
      [9, 25],
    ]) {
      const start = new Date(2026, month, day - 1, 22, 0)
      const midnight = new Date(2026, month, day).getTime()
      const nextMidnight = new Date(2026, month, day + 1).getTime()
      const key = localDateKey(new Date(2026, month, day))
      const allDayAndMore: MeetingLike = {
        timestamp: start.toISOString(),
        calendarDurationMinutes: 72 * 60,
        meetingWeight: 'busy',
      }

      expect(on(key, [allDayAndMore]).busy).toBe((nextMidnight - midnight) / 60_000)
    }
  })

  it('ignores entries that are not meetings or have no duration', () => {
    expect(
      on('2026-11-16', [
        meeting(16, 9, 0, 60, null),
        { ...meeting(16, 10, 0, 60), calendarDurationMinutes: null },
        meeting(16, 11, 0, 0),
      ])
    ).toEqual({ busy: 0, tentative: 0 })
  })

  it('has an entry for every requested day, empty ones included', () => {
    const days = meetingTimeByDay([], ['2026-11-16', '2026-11-17'])

    expect([...days.keys()]).toEqual(['2026-11-16', '2026-11-17'])
    expect(days.get('2026-11-17')).toEqual({ busyMinutes: 0, tentativeMinutes: 0 })
  })
})

describe('meeting time totals and labels', () => {
  it('sums days into a week', () => {
    expect(
      sumMeetingTime([
        { busyMinutes: 90, tentativeMinutes: 0 },
        { busyMinutes: 240, tentativeMinutes: 60 },
      ])
    ).toEqual({ busyMinutes: 330, tentativeMinutes: 60 })
  })

  it('reads busy time first and tentative time as a lighter extra', () => {
    expect(meetingTimeLabel({ busyMinutes: 330, tentativeMinutes: 60 })).toBe(
      '5h 30m · +1h tentative'
    )
    expect(meetingTimeLabel({ busyMinutes: 45, tentativeMinutes: 0 })).toBe('45m')
    expect(meetingTimeLabel({ busyMinutes: 0, tentativeMinutes: 30 })).toBe('30m tentative')
    expect(meetingTimeLabel({ busyMinutes: 0, tentativeMinutes: 0 })).toBeNull()
  })

  it('splits the label into parts for the headers', () => {
    expect(meetingTimeParts({ busyMinutes: 60, tentativeMinutes: 90 })).toEqual({
      busy: '1h',
      tentative: '+1h 30m',
    })
    expect(meetingTimeParts({ busyMinutes: 0, tentativeMinutes: 0 })).toBeNull()
  })
})
