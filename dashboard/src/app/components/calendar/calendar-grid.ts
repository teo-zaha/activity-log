// The calendar views' geometry: where entries sit on the hour grid of their local days, side by
// side where they overlap, and the bars of meetings that run past midnight. Pure, so the Day,
// Work week and Week views draw from one rule.

import type { BusyStatus, MeetingWeight } from '../../generated/activity'
import type { EntryKey } from '../../providers/connection'
import { busyMarkerOf } from '../../providers/utils'
import { type DayKey, wallClock } from '../../utils/local-day'

/**
 * The grid's rows: 24 hours, also on a day with 23 or 25 (daylight saving). Blocks sit by the
 * wall clock, so they line up with the hour labels and the current-time line every day.
 */
export const HOURS: readonly number[] = Array.from({ length: 24 }, (_, hour) => hour)
const DAY_MINUTES = HOURS.length * 60

/** The hour rows with their labels, worked out once. */
export const HOUR_ROWS: readonly { readonly hour: number; readonly label: string }[] = HOURS.map(
  hour => ({ hour, label: `${String(hour).padStart(2, '0')}:00` })
)

/** The shortest a block is drawn and laid out: room for one line of title, at every zoom. */
export const MIN_BLOCK_HEIGHT = 22
/** Where a calendar opens, scrolled: 06:30. */
export const DEFAULT_SCROLL_HOUR = 6.5

/** Between blocks side by side, and to the column's edges. */
const BLOCK_GAP = 4
const BAR_HEIGHT = 26
const BAR_GAP = 2
const BAR_SIDE_GAP = 3

export function gridHeight(hourHeight: number): number {
  return HOURS.length * hourHeight
}

/** What the grid reads of an entry; `EntryUiAdapter` has it. */
export interface GridEntry {
  readonly trackKey: EntryKey
  readonly timestamp: string
  readonly title: string
  /** A meeting's length; null for entries that are a moment. */
  readonly calendarDurationMinutes: number | null
  /** Null for entries that are no meetings. */
  readonly meetingWeight: MeetingWeight | null
  readonly busyStatus: BusyStatus | null
}

// ── Side by side: the sweep ─────────────────────────────────────────────────

/** An entry's extent on the grid in pixels, before the minimum height applies. */
export interface Span {
  readonly top: number
  readonly height: number
}

interface Placed<T> {
  readonly entry: T
  readonly top: number
  readonly height: number
  readonly end: number
  col: number
}

export interface LaidOutEvent<T> {
  readonly kind: 'event'
  readonly entry: T
  readonly top: number
  /** At least `MIN_BLOCK_HEIGHT`. */
  readonly height: number
  readonly col: number
  readonly cols: number
}

/** The entries a crowded cluster has no column for, as one block in its last column. */
export interface LaidOutOverflow<T> {
  readonly kind: 'overflow'
  readonly entries: readonly T[]
  readonly top: number
  readonly height: number
  readonly col: number
  readonly cols: number
}

export type LaidOut<T> = LaidOutEvent<T> | LaidOutOverflow<T>

/** A binary heap, smallest first by `before`. */
class MinHeap<T> {
  private readonly items: T[] = []

  constructor(private readonly before: (a: T, b: T) => boolean) {}

  get size(): number {
    return this.items.length
  }

  peek(): T | undefined {
    return this.items[0]
  }

  push(item: T): void {
    const items = this.items
    items.push(item)
    let i = items.length - 1
    while (i > 0) {
      const parent = (i - 1) >> 1
      if (!this.before(items[i], items[parent])) break
      ;[items[i], items[parent]] = [items[parent], items[i]]
      i = parent
    }
  }

  pop(): T | undefined {
    const items = this.items
    const top = items[0]
    const last = items.pop()
    if (items.length > 0 && last !== undefined) {
      items[0] = last
      let i = 0
      for (;;) {
        const [left, right] = [2 * i + 1, 2 * i + 2]
        let least = i
        if (left < items.length && this.before(items[left], items[least])) least = left
        if (right < items.length && this.before(items[right], items[least])) least = right
        if (least === i) break
        ;[items[i], items[least]] = [items[least], items[i]]
        i = least
      }
    }
    return top
  }
}

/**
 * Lays entries out side by side: by start, each takes the first column free at its top, and
 * every entry of a cluster (entries joined by overlaps) shares the cluster's column count, so
 * entries that overlap nothing take the whole width. A cluster wider than `maxVisible` keeps
 * `maxVisible - 1` columns and puts the rest into one overflow block in the last. Each entry is
 * at least `MIN_BLOCK_HEIGHT` tall, for the overlaps too. One sweep in start order: O(n log n).
 * The result lists each cluster's entries by start, its overflow last.
 */
export function layoutColumns<T>(
  entries: readonly T[],
  spanOf: (entry: T) => Span,
  maxVisible: number
): LaidOut<T>[] {
  const placed: Placed<T>[] = entries.map(entry => {
    const { top, height } = spanOf(entry)
    const drawn = Math.max(height, MIN_BLOCK_HEIGHT)
    return { entry, top, height: drawn, end: top + drawn, col: 0 }
  })
  // Stable: entries starting together keep the order they came in.
  placed.sort((a, b) => a.top - b.top)

  const result: LaidOut<T>[] = []
  const freeColumns = new MinHeap<number>((a, b) => a < b)
  const running = new MinHeap<Placed<T>>((a, b) => a.end < b.end)
  let columns = 0
  let cluster: Placed<T>[] = []
  let clusterEnd = -Infinity

  for (const item of placed) {
    if (item.top >= clusterEnd) {
      emitCluster(cluster, maxVisible, result)
      cluster = []
    }
    for (let done = running.peek(); done && done.end <= item.top; done = running.peek()) {
      running.pop()
      freeColumns.push(done.col)
    }
    item.col = freeColumns.pop() ?? columns++
    running.push(item)
    cluster.push(item)
    clusterEnd = Math.max(clusterEnd, item.end)
  }
  emitCluster(cluster, maxVisible, result)
  return result
}

function emitCluster<T>(
  cluster: readonly Placed<T>[],
  maxVisible: number,
  into: LaidOut<T>[]
): void {
  if (cluster.length === 0) return
  const cols = Math.max(...cluster.map(item => item.col)) + 1
  const event = (item: Placed<T>, shared: number): LaidOutEvent<T> => ({
    kind: 'event',
    entry: item.entry,
    top: item.top,
    height: item.height,
    col: item.col,
    cols: shared,
  })
  if (cols <= maxVisible) {
    for (const item of cluster) into.push(event(item, cols))
    return
  }
  const last = maxVisible - 1
  const hidden = cluster.filter(item => item.col >= last)
  for (const item of cluster) if (item.col < last) into.push(event(item, maxVisible))
  const top = Math.min(...hidden.map(item => item.top))
  into.push({
    kind: 'overflow',
    entries: hidden.map(item => item.entry),
    top,
    height: Math.max(...hidden.map(item => item.end)) - top,
    col: last,
    cols: maxVisible,
  })
}

// ── Days: blocks and bars ───────────────────────────────────────────────────

/** An entry drawn as a block on its day, by the wall clock. */
interface TimedEntry<T> {
  readonly entry: T
  readonly startMinutes: number
  /**
   * Where the block ends, by the wall clock; 24:00 for one that ends at midnight. Before the
   * start for a meeting across the autumn change (02:40 CEST to 02:10 CET): the minimum height.
   */
  readonly endMinutes: number
}

/** A meeting past midnight, as a bar over the days it covers. */
interface SpanningEntry<T> {
  readonly entry: T
  readonly firstDay: DayKey
  readonly lastDay: DayKey
}

/**
 * The entries of `days` placed on them, by the wall clock in `timeZone` (an IANA name; the
 * browser's zone without one). Does not depend on the zoom, so zooming only lays out again.
 */
export interface CalendarPlan<T> {
  readonly days: readonly DayKey[]
  readonly timed: ReadonlyMap<DayKey, readonly TimedEntry<T>[]>
  readonly spanning: readonly SpanningEntry<T>[]
}

export function planCalendar<T extends GridEntry>(
  days: readonly DayKey[],
  entries: readonly T[],
  timeZone?: string
): CalendarPlan<T> {
  const timed = new Map<DayKey, TimedEntry<T>[]>(days.map(day => [day, []]))
  const spanning: SpanningEntry<T>[] = []
  const [firstShown, lastShown] = [days[0], days[days.length - 1]]
  for (const entry of entries) {
    const start = Date.parse(entry.timestamp)
    const minutes = Math.max(entry.calendarDurationMinutes ?? 0, 0)
    const end = start + minutes * 60_000
    const startClock = wallClock(start, timeZone)
    const lastDay = minutes > 0 ? wallClock(end - 1, timeZone).day : startClock.day
    // Meetings are what the backend weighs for meeting time, `none` included.
    if (entry.meetingWeight !== null && lastDay > startClock.day) {
      if (startClock.day <= lastShown && lastDay >= firstShown) {
        spanning.push({ entry, firstDay: startClock.day, lastDay })
      }
      continue
    }
    const endClock = wallClock(end, timeZone)
    timed.get(startClock.day)?.push({
      entry,
      startMinutes: startClock.minutes,
      endMinutes: endClock.day === startClock.day ? endClock.minutes : DAY_MINUTES,
    })
  }
  return { days, timed, spanning }
}

/** Where a block or bar sits across its column, as CSS lengths. */
interface Across {
  readonly left: string
  readonly width: string
}

export interface CalendarBlock<T> extends Across {
  readonly kind: 'event'
  readonly key: EntryKey
  readonly entry: T
  readonly top: number
  readonly height: number
  /** `cal-event`, with the busy marker. */
  readonly className: string
  readonly label: string
}

export interface CalendarOverflow<T> extends Across {
  readonly kind: 'overflow'
  readonly key: string
  readonly entries: readonly T[]
  readonly keys: readonly EntryKey[]
  readonly top: number
  readonly height: number
  readonly label: string
}

export type ColumnItem<T> = CalendarBlock<T> | CalendarOverflow<T>

export interface DayColumn<T> {
  readonly day: DayKey
  readonly items: readonly ColumnItem<T>[]
}

/** The day columns of a plan at `hourHeight`, a cluster at most `maxVisible` columns wide. */
export function layoutDays<T extends GridEntry>(
  plan: CalendarPlan<T>,
  hourHeight: number,
  maxVisible: number
): DayColumn<T>[] {
  const px = (minutes: number): number => (minutes / 60) * hourHeight
  return plan.days.map(day => {
    const laidOut = layoutColumns(
      plan.timed.get(day) ?? [],
      ({ startMinutes, endMinutes }) => ({
        top: px(startMinutes),
        height: px(endMinutes - startMinutes),
      }),
      maxVisible
    )
    return {
      day,
      items: laidOut.map((item): ColumnItem<T> => {
        const across = blockAcross(item.col, item.cols)
        if (item.kind === 'overflow') {
          const keys = item.entries.map(timedEntry => timedEntry.entry.trackKey)
          return {
            kind: 'overflow',
            key: `overflow:${keys.join('|')}`,
            entries: item.entries.map(timedEntry => timedEntry.entry),
            keys,
            top: item.top,
            height: item.height,
            label: `Open ${keys.length} additional events`,
            ...across,
          }
        }
        const entry = item.entry.entry
        return {
          kind: 'event',
          key: entry.trackKey,
          entry,
          top: item.top,
          height: item.height,
          className: calendarEventClass('cal-event', entry),
          label: calendarEventLabel(entry),
          ...across,
        }
      }),
    }
  })
}

function blockAcross(col: number, cols: number): Across {
  const share = 100 / cols
  return {
    left: `calc(${col * share}% + ${BLOCK_GAP}px)`,
    width: `calc(${share}% - ${BLOCK_GAP * 2}px)`,
  }
}

export interface AllDayBar<T> extends Across {
  readonly key: EntryKey
  readonly entry: T
  readonly top: number
  readonly height: number
  /** `cal-allday-bar`, with the busy marker. */
  readonly className: string
  readonly label: string
}

/** The bars above the grid, and the days they cover. */
export interface AllDayRow<T> {
  readonly bars: readonly AllDayBar<T>[]
  /** 0 without bars. */
  readonly height: number
  readonly days: ReadonlySet<DayKey>
}

/**
 * The bars of a plan's meetings past midnight, clipped to its days. Packed greedily by first
 * day, longer bars first: as many rows as bars overlap on one day, whatever order they came in.
 */
export function layoutAllDay<T extends GridEntry>(plan: CalendarPlan<T>): AllDayRow<T> {
  const days = plan.days
  const [firstShown, lastShown] = [days[0], days[days.length - 1]]
  const bars = plan.spanning.map(({ entry, firstDay, lastDay }) => {
    const startCol = days.indexOf(firstDay < firstShown ? firstShown : firstDay)
    const endCol = days.indexOf(lastDay > lastShown ? lastShown : lastDay)
    return { entry, startCol, spanCols: endCol - startCol + 1 }
  })
  bars.sort((a, b) => a.startCol - b.startCol || b.spanCols - a.spanCols)

  const rowEnds: number[] = []
  const covered = new Set<DayKey>()
  const positioned = bars.map(({ entry, startCol, spanCols }): AllDayBar<T> => {
    let row = rowEnds.findIndex(end => end <= startCol)
    if (row === -1) row = rowEnds.length
    rowEnds[row] = startCol + spanCols
    for (const day of days.slice(startCol, startCol + spanCols)) covered.add(day)
    return {
      key: entry.trackKey,
      entry,
      left: `calc(${(startCol / days.length) * 100}% + ${BAR_SIDE_GAP}px)`,
      width: `calc(${(spanCols / days.length) * 100}% - ${BAR_SIDE_GAP * 2}px)`,
      top: row * (BAR_HEIGHT + BAR_GAP) + BAR_GAP,
      height: BAR_HEIGHT,
      className: calendarEventClass('cal-allday-bar', entry),
      label: calendarEventLabel(entry),
    }
  })
  return {
    bars: positioned,
    height: rowEnds.length === 0 ? 0 : rowEnds.length * (BAR_HEIGHT + BAR_GAP) + BAR_GAP * 2,
    days: covered,
  }
}

// ── Markers and labels ──────────────────────────────────────────────────────

/** `cal-event is-tentative`: the busy marker; the colours come from `data-src`. */
export function calendarEventClass(
  base: string,
  entry: { readonly busyStatus: BusyStatus | null }
): string {
  const marker = busyMarkerOf(entry.busyStatus)
  return marker ? `${base} is-${marker}` : base
}

/** `Open event: Sync (tentative)`: the marker's meaning for screen readers too. */
export function calendarEventLabel(entry: {
  readonly title: string
  readonly busyStatus: BusyStatus | null
}): string {
  const marker = busyMarkerOf(entry.busyStatus)
  return `Open event: ${entry.title}${marker ? ` (${marker})` : ''}`
}

export interface NowMarker {
  /** The day the line is drawn on. */
  readonly day: DayKey
  readonly top: number
  readonly label: string
  /** The hour whose gutter label the current-time label would overlap, hidden; or null. */
  readonly coveredHour: number | null
}

/**
 * The gutter labels, in px from their slot's or line's top: an hour label takes 4–16 px of its
 * slot, the current-time label 9 px above to 7 px below the line. 2 px apart is still a clash.
 */
const HOUR_LABEL = { top: 4, bottom: 16 } as const
const NOW_LABEL = { above: 9, below: 7 } as const
const LABEL_GAP = 2

/** Where the current-time line sits, by the wall clock as the blocks are. */
export function nowMarker(now: number, hourHeight: number, timeZone?: string): NowMarker {
  const { day, minutes } = wallClock(now, timeZone)
  const top = (minutes / 60) * hourHeight
  const hour = Math.floor(minutes / 60)
  // The clash zone around an hour is narrower than the smallest zoom's hour, so at most one hour
  // label clashes: this one or the next.
  const coveredHour =
    [hour, hour + 1].find(candidate => {
      const slot = candidate * hourHeight
      return (
        top - NOW_LABEL.above < slot + HOUR_LABEL.bottom + LABEL_GAP &&
        top + NOW_LABEL.below > slot + HOUR_LABEL.top - LABEL_GAP
      )
    }) ?? null
  const clock = (value: number): string => String(value).padStart(2, '0')
  return { day, top, label: `${clock(hour)}:${clock(minutes % 60)}`, coveredHour }
}
