import { describe, expect, it } from 'vitest'
import type { BusyStatus } from '../generated/activity'
import {
  calendarEntryKey,
  calendarEventClass,
  calendarEventLabel,
  layoutEventsWithOverflow,
  nowMarker,
} from './calendar-layout'

interface TestEntry {
  id: string
  source: string
  timestamp: string
  title: string
  top: number
  height: number
}

const entry = (id: string, top: number, height = 30): TestEntry => ({
  id,
  source: 'git',
  timestamp: `2026-05-18T09:${id.padStart(2, '0')}:00Z`,
  title: `Entry ${id}`,
  top,
  height,
})

describe('calendar layout', () => {
  it('keeps non-overlapping entries in one column', () => {
    const layout = layoutEventsWithOverflow(
      [entry('1', 0), entry('2', 40)],
      item => item.top,
      item => item.height
    )

    expect(layout).toMatchObject([
      { kind: 'event', col: 0, cols: 1 },
      { kind: 'event', col: 0, cols: 1 },
    ])
  })

  it('assigns overlapping entries to separate columns', () => {
    const layout = layoutEventsWithOverflow(
      [entry('1', 0, 50), entry('2', 10, 50)],
      item => item.top,
      item => item.height
    )

    expect(layout).toMatchObject([
      { kind: 'event', col: 0, cols: 2 },
      { kind: 'event', col: 1, cols: 2 },
    ])
  })

  it('collapses excess overlap columns into overflow', () => {
    const layout = layoutEventsWithOverflow(
      [entry('1', 0, 50), entry('2', 5, 50), entry('3', 10, 50)],
      item => item.top,
      item => item.height,
      22,
      2
    )

    expect(layout.some(item => item.kind === 'overflow')).toBe(true)
  })

  it('keys an entry by the id the backend gave it', () => {
    expect(calendarEntryKey({ trackKey: 'git/laptop/abc123' })).toBe('git/laptop/abc123')
  })
})

describe('calendarEventClass', () => {
  it('adds a marker for tentative and free meetings only', () => {
    const cls = (busyStatus: BusyStatus | null): string =>
      calendarEventClass('cal-event', { busyStatus })

    expect(cls('tentative')).toBe('cal-event is-tentative')
    expect(cls('free')).toBe('cal-event is-free')
    expect(cls('busy')).toBe('cal-event')
    expect(cls('out_of_office')).toBe('cal-event')
    expect(cls(null)).toBe('cal-event')
  })
})

describe('nowMarker', () => {
  it('places the line by local wall-clock time', () => {
    const marker = nowMarker(new Date(2026, 10, 17, 14, 30).getTime(), 64)

    expect(marker).toEqual({ top: 14.5 * 64, label: '14:30', coveredHour: null })
  })

  it('hides the hour label the time label would overlap, on and around the full hour', () => {
    const covered = (hours: number, minutes: number, hourHeight = 64): number | null =>
      nowMarker(new Date(2026, 10, 17, hours, minutes).getTime(), hourHeight).coveredHour

    expect(covered(14, 0)).toBe(14)
    expect(covered(14, 20)).toBe(14)
    expect(covered(13, 58)).toBe(14)
    expect(covered(13, 50)).toBeNull()
    expect(covered(14, 30)).toBeNull()
    // Zoomed out, the next hour's label is a few minutes away.
    expect(covered(14, 52, 32)).toBe(15)
    expect(covered(14, 30, 32)).toBe(14)
  })
})

describe('calendarEventLabel', () => {
  it('names the status of tentative and free meetings', () => {
    const label = (busyStatus: BusyStatus | null): string =>
      calendarEventLabel({ title: 'Sync', busyStatus })

    expect(label('tentative')).toBe('Open event: Sync (tentative)')
    expect(label('free')).toBe('Open event: Sync (free)')
    expect(label('busy')).toBe('Open event: Sync')
    expect(label(null)).toBe('Open event: Sync')
  })
})
