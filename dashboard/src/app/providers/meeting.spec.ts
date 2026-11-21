import { describe, expect, it } from 'vitest'
import type { CalendarEntry, OutlookEntry } from '../generated/activity'
import { meetingInit } from './meeting'
import { createEntryAdapter } from './provider-adapter'

const body = {
  connection: 'default',
  id: 'review',
  timestamp: '2026-11-17T09:00:00Z',
  title: 'Design review',
  location: 'Room 4',
  organizer: 'Ada',
  durationMinutes: 90,
  busyStatus: 'tentative',
  meetingWeight: 'tentative',
} as const

const calendar: CalendarEntry = { ...body, source: 'calendar' }
const outlook = (isOrganizer: boolean | undefined): OutlookEntry => ({
  ...body,
  source: 'outlook',
  isOrganizer,
})

const fields = (entry: CalendarEntry | OutlookEntry): Record<string, string> =>
  Object.fromEntries(createEntryAdapter(entry).detailFields.map(f => [f.label, f.value]))

describe('meetingInit', () => {
  it('shows a meeting of either source the same way, but for the role', () => {
    const fromCalendar = meetingInit({ source: 'calendar', label: 'Calendar' }, calendar, null)
    const fromOutlook = meetingInit(
      { source: 'outlook', label: 'Outlook' },
      outlook(undefined),
      null
    )

    const { badge, badges, styleKey, ...rest } = fromCalendar
    expect(fromOutlook).toEqual({
      ...rest,
      badge: { ...badge, source: 'outlook', label: 'Outlook' },
      badges: [{ ...badge, source: 'outlook', label: 'Outlook' }],
      styleKey: 'outlook',
    })
    expect(fromCalendar.meta).toBe('1h 30m · Room 4 · org: Ada')
    expect(styleKey).toBe('calendar')
    expect(badges).toEqual([badge])
  })

  it('names a meeting without a subject by the placeholder', () => {
    const init = meetingInit(
      { source: 'calendar', label: 'Calendar' },
      { ...calendar, title: undefined },
      null
    )

    expect(init.title).toBe('(No title)')
    expect(init.detailFields[0]).toEqual({ label: 'Title', value: '(No title)' })
  })
})

describe('Outlook meetings', () => {
  it('hide the organizer when the user organizes it', () => {
    const adapter = createEntryAdapter(outlook(true))

    expect(adapter.meta).toBe('1h 30m · Room 4 · organizer')
    expect(fields(outlook(true))).toMatchObject({ Organizer: 'Ada', Role: 'Organizer' })
  })

  it('name the organizer of a meeting the user attends', () => {
    const adapter = createEntryAdapter(outlook(false))

    expect(adapter.meta).toBe('1h 30m · Room 4 · attendee · org: Ada')
    expect(fields(outlook(false))).toMatchObject({ Organizer: 'Ada', Role: 'Attendee' })
  })

  it('show no role where Outlook did not say', () => {
    const adapter = createEntryAdapter(outlook(undefined))

    expect(adapter.meta).toBe('1h 30m · Room 4 · org: Ada')
    expect(fields(outlook(undefined))).not.toHaveProperty('Role')
  })
})

describe('calendar meetings', () => {
  it('have no role, and name their organizer', () => {
    expect(createEntryAdapter(calendar).meta).toBe('1h 30m · Room 4 · org: Ada')
    expect(fields(calendar)).toEqual({
      Title: 'Design review',
      Duration: '1h 30m',
      Status: 'Tentative',
      Location: 'Room 4',
      Organizer: 'Ada',
    })
  })
})
