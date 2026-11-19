import { describe, expect, it } from 'vitest'
import type { CalendarEntry, OutlookEntry } from '../../generated/activity'
import { createEntryAdapter } from '../provider-adapter'

const calendar: CalendarEntry = {
  source: 'calendar',
  connection: 'default',
  id: 'planning',
  timestamp: '2026-11-17T09:00:00Z',
  title: 'Planning',
  durationMinutes: 30,
  meetingWeight: 'busy',
  description: 'Review the Budget for Q3',
  joinUrl: 'https://meet.google.com/abc-defg-hij',
}

const outlook: OutlookEntry = {
  source: 'outlook',
  connection: 'default',
  id: 'planning',
  timestamp: '2026-11-17T09:00:00Z',
  title: 'Planning',
  isOrganizer: true,
  durationMinutes: 30,
  meetingWeight: 'busy',
  description: 'Review the Budget for Q3',
  joinUrl: 'https://zoom.us/j/1',
}

describe('meeting adapters', () => {
  it.each([calendar, outlook])('carry the description and join link of $source', entry => {
    const adapter = createEntryAdapter(entry)

    expect(adapter.description).toBe('Review the Budget for Q3')
    expect(adapter.joinUrl).toBe(entry.joinUrl)
    expect(adapter.searchText).toContain('budget for q3')
  })

  it('leaves both empty for a meeting without them', () => {
    const adapter = createEntryAdapter({
      ...calendar,
      description: undefined,
      joinUrl: undefined,
    })

    expect(adapter.description).toBeNull()
    expect(adapter.joinUrl).toBeNull()
  })

  it('are tracked by the id the backend gave them, within their connection', () => {
    const first = createEntryAdapter(calendar)
    const sameLooks = createEntryAdapter({ ...calendar, id: 'planning#2' })
    const otherConnection = createEntryAdapter({ ...calendar, connection: 'team' })

    expect(first.trackKey).toBe('calendar/default/planning')
    expect(sameLooks.trackKey).not.toBe(first.trackKey)
    expect(otherConnection.trackKey).toBe('calendar/team/planning')
  })
})
