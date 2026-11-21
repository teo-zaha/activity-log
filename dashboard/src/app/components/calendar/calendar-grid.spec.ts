import { describe, expect, it } from 'vitest'
import type { BusyStatus, MeetingWeight } from '../../generated/activity'
import type { EntryKey } from '../../providers/connection'
import {
  MIN_BLOCK_HEIGHT,
  calendarEventClass,
  calendarEventLabel,
  layoutAllDay,
  layoutColumns,
  layoutDays,
  nowMarker,
  planCalendar,
  type CalendarBlock,
  type GridEntry,
} from './calendar-grid'

interface TestEntry {
  id: string
  top: number
  height: number
}

const entry = (id: string, top: number, height = 30): TestEntry => ({ id, top, height })
const span = (item: TestEntry): TestEntry => item

// Cases from the layout this module replaced (`calendar-layout.ts`), results unchanged.
describe('layoutColumns, as the layout it replaced', () => {
  it('keeps non-overlapping entries in one column', () => {
    const layout = layoutColumns([entry('1', 0), entry('2', 40)], span, 4)

    expect(layout).toMatchObject([
      { kind: 'event', col: 0, cols: 1 },
      { kind: 'event', col: 0, cols: 1 },
    ])
  })

  it('assigns overlapping entries to separate columns', () => {
    const layout = layoutColumns([entry('1', 0, 50), entry('2', 10, 50)], span, 4)

    expect(layout).toMatchObject([
      { kind: 'event', col: 0, cols: 2 },
      { kind: 'event', col: 1, cols: 2 },
    ])
  })

  it('collapses excess overlap columns into overflow', () => {
    const layout = layoutColumns(
      [entry('1', 0, 50), entry('2', 5, 50), entry('3', 10, 50)],
      span,
      2
    )

    expect(layout.some(item => item.kind === 'overflow')).toBe(true)
  })
})

/** `[id, col, cols]` of the events, `['+ids', col, cols]` of an overflow. */
function columnsOf(
  layout: ReturnType<typeof layoutColumns<TestEntry>>
): [string, number, number][] {
  return layout.map(item =>
    item.kind === 'event'
      ? [item.entry.id, item.col, item.cols]
      : [`+${item.entries.map(e => e.id).join(',')}`, item.col, item.cols]
  )
}

describe('layoutColumns', () => {
  it('reuses the first free column and widens the whole cluster', () => {
    // A runs long; B ends before D starts, so D takes B's column; C chains them all.
    const layout = layoutColumns(
      [entry('A', 0, 200), entry('B', 10, 40), entry('C', 20, 100), entry('D', 60, 40)],
      span,
      5
    )

    expect(columnsOf(layout)).toEqual([
      ['A', 0, 3],
      ['B', 1, 3],
      ['C', 2, 3],
      ['D', 1, 3],
    ])
  })

  it('treats touching entries as apart, and lays out clusters on their own', () => {
    const layout = layoutColumns(
      [entry('A', 0, 40), entry('B', 10, 30), entry('C', 40, 30), entry('D', 100, 30)],
      span,
      4
    )

    expect(columnsOf(layout)).toEqual([
      ['A', 0, 2],
      ['B', 1, 2],
      ['C', 0, 1],
      ['D', 0, 1],
    ])
  })

  it('keeps the given order of entries that start together', () => {
    const layout = layoutColumns([entry('A', 30, 30), entry('B', 30, 30), entry('C', 30)], span, 4)

    expect(columnsOf(layout)).toEqual([
      ['A', 0, 3],
      ['B', 1, 3],
      ['C', 2, 3],
    ])
  })

  it('lays out short and zero-length entries at the minimum height', () => {
    // 5 px apart: at their real height they would not overlap.
    const layout = layoutColumns([entry('A', 0, 0), entry('B', 5, 4), entry('C', 30, 0)], span, 4)

    expect(columnsOf(layout)).toEqual([
      ['A', 0, 2],
      ['B', 1, 2],
      ['C', 0, 1],
    ])
    expect(layout.map(item => item.height)).toEqual([22, 22, 22])
    expect(MIN_BLOCK_HEIGHT).toBe(22)
  })

  it('draws an entry whose end lies before its start at the minimum height', () => {
    const [item] = layoutColumns([entry('A', 100, -40)], span, 4)

    expect(item).toMatchObject({ kind: 'event', top: 100, height: MIN_BLOCK_HEIGHT })
  })

  it('puts every entry past the last visible column into one overflow', () => {
    // Six at once, as the Week view draws them (three columns).
    const layout = layoutColumns(
      [
        entry('A', 0, 120),
        entry('B', 15, 60),
        entry('C', 30, 60),
        entry('D', 30, 45),
        entry('E', 60, 30),
        entry('F', 75, 45),
      ],
      span,
      3
    )

    expect(columnsOf(layout)).toEqual([
      ['A', 0, 3],
      ['B', 1, 3],
      ['F', 1, 3],
      ['+C,D,E', 2, 3],
    ])
    const overflow = layout[3]
    expect(overflow).toMatchObject({ kind: 'overflow', top: 30, height: 60 })
  })

  it('lays out 2000 overlapping entries in one pass', () => {
    // Each starts 1 px after the one before and lasts 50 px: 50 columns, one cluster.
    const many = Array.from({ length: 2000 }, (_, i) => entry(String(i), i, 50))

    const layout = layoutColumns(many, span, 2000)

    expect(layout).toHaveLength(2000)
    expect(layout.every(item => item.kind === 'event' && item.cols === 50)).toBe(true)
    expect(columnsOf(layout).slice(48, 52)).toEqual([
      ['48', 48, 50],
      ['49', 49, 50],
      ['50', 0, 50],
      ['51', 1, 50],
    ])
  })
})

// ── Days, in a fixed zone: the test runner may run in any ─────────────────

const ZONE = 'Europe/Berlin'
let ids = 0

function meeting(
  timestamp: string,
  minutes: number | null,
  weight: MeetingWeight | null = 'busy',
  busyStatus: BusyStatus | null = null
): GridEntry {
  ids += 1
  return {
    trackKey: `outlook/default/m${ids}` satisfies EntryKey,
    timestamp,
    title: `Meeting ${ids}`,
    calendarDurationMinutes: minutes,
    meetingWeight: weight,
    busyStatus,
  }
}

function blocksOf(columns: ReturnType<typeof layoutDays<GridEntry>>): CalendarBlock<GridEntry>[] {
  return columns.flatMap(column =>
    column.items.flatMap(item => (item.kind === 'event' ? [item] : []))
  )
}

describe('planCalendar and layoutDays', () => {
  const HOUR = 64

  it('places blocks by the wall clock on their day', () => {
    const planning = meeting('2026-11-17T09:30:00+01:00', 90)
    const plan = planCalendar(['2026-11-16', '2026-11-17'], [planning], ZONE)

    const [monday, tuesday] = layoutDays(plan, HOUR, 4)

    expect(monday.items).toEqual([])
    expect(tuesday.items).toMatchObject([
      { kind: 'event', key: planning.trackKey, top: 9.5 * HOUR, height: 1.5 * HOUR },
    ])
  })

  it('ends a block on the row of its end time on the spring-forward day', () => {
    // 28 March 2027: 01:30 CET to 03:30 CEST is one real hour, two on the clock.
    const night = meeting('2027-03-28T01:30:00+01:00', 60)
    const after = meeting('2027-03-28T04:00:00+02:00', 60)

    const [day] = layoutDays(planCalendar(['2027-03-28'], [night, after], ZONE), HOUR, 4)

    const [first, second] = blocksOf([day])
    expect(first.top).toBe(1.5 * HOUR)
    expect(first.top + first.height).toBe(3.5 * HOUR)
    expect(second).toMatchObject({ top: 4 * HOUR, height: HOUR })
  })

  it('draws the autumn change by the wall clock, never shorter than the minimum', () => {
    // 25 October 2026: the clock reads 02:00–03:00 twice.
    const across = meeting('2026-10-25T01:30:00+02:00', 120) // 01:30 CEST to 02:30 CET
    const inside = meeting('2026-10-25T02:40:00+02:00', 30) // 02:40 CEST to 02:10 CET
    const repeated = meeting('2026-10-25T02:00:00+01:00', 60) // the second 02:00 to 03:00

    const blocks = blocksOf(
      layoutDays(planCalendar(['2026-10-25'], [across, inside, repeated], ZONE), HOUR, 4)
    )
    const block = (entry: GridEntry): CalendarBlock<GridEntry> | undefined =>
      blocks.find(item => item.entry === entry)

    expect(block(across)).toMatchObject({ top: 1.5 * HOUR, height: HOUR })
    expect(block(inside)).toMatchObject({ top: (2 + 40 / 60) * HOUR, height: MIN_BLOCK_HEIGHT })
    expect(block(repeated)).toMatchObject({ top: 2 * HOUR, height: HOUR })
  })

  it('draws a moment at the minimum height and a meeting to midnight as a block', () => {
    const commit = meeting('2026-11-17T13:05:00+01:00', null, null)
    const late = meeting('2026-11-17T23:00:00+01:00', 60)

    const blocks = blocksOf(layoutDays(planCalendar(['2026-11-17'], [commit, late], ZONE), HOUR, 4))

    expect(blocks.map(block => [block.top, block.height])).toEqual([
      [(13 + 5 / 60) * HOUR, MIN_BLOCK_HEIGHT],
      [23 * HOUR, HOUR],
    ])
  })

  it('draws meetings past midnight as bars over the days they cover, clipped', () => {
    const tuesdayNight = meeting('2026-11-17T22:00:00+01:00', 4 * 60)
    const fromLastWeek = meeting('2026-11-14T09:00:00+01:00', 2 * 24 * 60)
    // Not a meeting: a moment, whatever its day.
    const commit = meeting('2026-11-18T01:00:00+01:00', null, null)
    const week = ['2026-11-16', '2026-11-17', '2026-11-18', '2026-11-19'] as const

    const plan = planCalendar(week, [tuesdayNight, fromLastWeek, commit], ZONE)
    const allDay = layoutAllDay(plan)

    expect(allDay.bars.map(bar => [bar.entry, bar.left, bar.width, bar.top])).toEqual([
      [fromLastWeek, 'calc(0% + 3px)', 'calc(25% - 6px)', 2],
      [tuesdayNight, 'calc(25% + 3px)', 'calc(50% - 6px)', 2],
    ])
    expect(allDay.height).toBe(32)
    expect([...allDay.days]).toEqual(['2026-11-16', '2026-11-17', '2026-11-18'])
    expect(blocksOf(layoutDays(plan, HOUR, 4)).map(block => block.entry)).toEqual([commit])
  })

  it('keeps a meeting of weight none a meeting: it can be a bar', () => {
    const free = meeting('2026-11-17T22:00:00+01:00', 4 * 60, 'none', 'free')

    const allDay = layoutAllDay(planCalendar(['2026-11-17', '2026-11-18'], [free], ZONE))

    expect(allDay.bars).toMatchObject([
      { className: 'cal-allday-bar is-free', label: `Open event: ${free.title} (free)` },
    ])
  })

  it('packs bars into as few rows as overlap on one day, whatever their order', () => {
    // Mon–Tue, Tue–Wed and Wed–Thu, newest first as the log lists them: two rows do.
    const wed = meeting('2026-11-18T20:00:00+01:00', 25 * 60)
    const tue = meeting('2026-11-17T20:00:00+01:00', 25 * 60)
    const mon = meeting('2026-11-16T20:00:00+01:00', 25 * 60)
    const week = ['2026-11-16', '2026-11-17', '2026-11-18', '2026-11-19'] as const

    const { bars, height } = layoutAllDay(planCalendar(week, [wed, tue, mon], ZONE))

    const row = (entry: GridEntry): number | undefined => bars.find(bar => bar.entry === entry)?.top
    expect(row(mon)).toBe(row(wed))
    expect(row(tue)).not.toBe(row(mon))
    expect(height).toBe(2 * 28 + 4)
  })

  it('names the blocks of a crowded cluster and its overflow for people', () => {
    const at = (time: string, status: BusyStatus | null = null): GridEntry =>
      meeting(`2026-11-17T${time}:00+01:00`, 60, 'busy', status)
    const crowd = [at('09:00', 'tentative'), at('09:15'), at('09:30'), at('09:45')]

    const [column] = layoutDays(planCalendar(['2026-11-17'], crowd, ZONE), HOUR, 3)

    expect(column.items).toMatchObject([
      {
        kind: 'event',
        className: 'cal-event is-tentative',
        label: `Open event: ${crowd[0].title} (tentative)`,
        left: 'calc(0% + 4px)',
        width: 'calc(33.333333333333336% - 8px)',
      },
      { kind: 'event', className: 'cal-event', key: crowd[1].trackKey },
      {
        kind: 'overflow',
        keys: [crowd[2].trackKey, crowd[3].trackKey],
        label: 'Open 2 additional events',
        top: 9.5 * HOUR,
        height: 1.25 * HOUR,
      },
    ])
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

    expect(marker).toEqual({ day: '2026-11-17', top: 14.5 * 64, label: '14:30', coveredHour: null })
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

  it('reads the clock as the blocks do on a daylight saving day', () => {
    // 03:45 CEST on the spring-forward day: the row a block from 03:30 to 04:00 covers.
    const marker = nowMarker(Date.parse('2027-03-28T03:45:00+02:00'), 64, ZONE)

    expect(marker).toEqual({ day: '2027-03-28', top: 3.75 * 64, label: '03:45', coveredHour: null })
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
