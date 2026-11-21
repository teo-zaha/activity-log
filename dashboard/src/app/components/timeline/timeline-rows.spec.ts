import { describe, expect, it } from 'vitest'
import type { DayGroup } from '../../models/day-group'
import { createEntryAdapter } from '../../providers/provider-adapter'
import type { EntryUiAdapter } from '../../providers/types'
import { activityLog } from '../../testing/activity-fixtures'
import type { DayKey } from '../../utils/local-day'
import { TimelineRows, timelineRow } from './timeline-rows'

const [git, jira] = activityLog.entries.map(createEntryAdapter)

const meeting = (
  title: string,
  busyStatus: 'tentative' | 'busy',
  joinUrl?: string
): EntryUiAdapter =>
  createEntryAdapter({
    source: 'outlook',
    connection: 'default',
    id: title,
    timestamp: '2026-05-18T09:00:00Z',
    title,
    durationMinutes: 30,
    busyStatus,
    meetingWeight: busyStatus,
    joinUrl,
  })

/** Fresh group objects each time, as each answer and each filter makes them. */
const groups = (...days: [DayKey, EntryUiAdapter[]][]): DayGroup<EntryUiAdapter>[] =>
  days.map(([date, entries]) => ({ date, entries: [...entries] }))

describe('timelineRow', () => {
  it('names the source, the title, the meeting status and the time', () => {
    const call = timelineRow(meeting('Call', 'tentative', 'https://zoom.us/j/1'))

    expect(call.ariaLabel).toMatch(/^Open Outlook event: Call \(tentative, online meeting\) at /)
    expect(call.busy).toEqual({ label: 'Tentative', className: 'busy-tag is-tentative' })
  })

  it('tags no busy meeting', () => {
    const sync = timelineRow(meeting('Sync', 'busy'))

    expect(sync.busy).toBeNull()
    expect(sync.ariaLabel).toMatch(/event: Sync at /)
  })
})

describe('TimelineRows', () => {
  it('keeps an entry’s row across answers and filters', () => {
    const rows = new TimelineRows()

    const [first] = rows.days(groups(['2026-05-18', [git, jira]]), '2026-05-18')
    const [again] = rows.days(groups(['2026-05-18', [jira]]), '2026-05-18')

    expect(again?.rows[0]).toBe(first?.rows[1])
  })

  it('keeps a day whose rows are the same objects, while another day changes', () => {
    const rows = new TimelineRows()
    const before = rows.days(groups(['2026-05-18', [jira]], ['2026-05-17', [git]]), '2026-05-18')

    const after = rows.days(
      groups(['2026-05-18', [jira, meeting('New', 'busy')]], ['2026-05-17', [git]]),
      '2026-05-18'
    )

    expect(after[1]).toBe(before[1])
    expect(after[0]).not.toBe(before[0])
    expect(after[0]?.count).toBe('2 events')
  })

  it('makes a new day when its relative label changes at midnight', () => {
    const rows = new TimelineRows()
    const [today] = rows.days(groups(['2026-05-18', [git]]), '2026-05-18')

    const [yesterday] = rows.days(groups(['2026-05-18', [git]]), '2026-05-19')
    const [older] = rows.days(groups(['2026-05-18', [git]]), '2026-05-20')

    expect(today).toMatchObject({
      relative: 'Today',
      label: 'Monday, 18 May 2026',
      count: '1 event',
    })
    expect(yesterday).not.toBe(today)
    expect(yesterday?.relative).toBe('Yesterday')
    expect(yesterday?.rows[0]).toBe(today?.rows[0])
    expect(older?.relative).toBeNull()
  })

  it('forgets days no longer drawn', () => {
    const rows = new TimelineRows()
    const [first] = rows.days(groups(['2026-05-18', [git]]), '2026-05-18')
    rows.days([], '2026-05-18')

    const [back] = rows.days(groups(['2026-05-18', [git]]), '2026-05-18')

    expect(back).not.toBe(first)
    expect(back?.rows[0]).toBe(first?.rows[0])
  })
})
