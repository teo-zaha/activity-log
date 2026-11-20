import type {
  DayIndexDay,
  DayIndexDensity,
  DayIndexItem,
  DayIndexMonthRow,
  MonthDensity,
} from '../models/day-index'
import { addDaysToKey, dateOfKey, type DayKey, monthLabel } from './local-day'

/**
 * Every day from `toKey` back to `fromKey` (newest first, like the timeline), with a month
 * heading before each month's first row. A heading where the year changes shows the year, and
 * when the range spans years the oldest heading shows its month and year (`Nov ’25`).
 */
export function buildDayIndex(
  fromKey: DayKey,
  toKey: DayKey,
  counts: ReadonlyMap<DayKey, number>,
  todayKey: DayKey
): DayIndexItem[] {
  if (fromKey > toKey) return []
  const max = Math.max(1, ...counts.values())
  const items: DayIndexItem[] = []
  const shortYear = (date: Date): string => `’${String(date.getFullYear()).slice(2)}`
  let previous: Date | null = null
  for (let key = toKey; key >= fromKey; key = addDaysToKey(key, -1)) {
    const date = dateOfKey(key)
    if (previous?.getMonth() !== date.getMonth()) {
      const yearChanged = previous !== null && previous.getFullYear() !== date.getFullYear()
      items.push({
        kind: 'month',
        key: key.slice(0, 7),
        day: key,
        label: yearChanged ? `${monthLabel(key)} ${shortYear(date)}` : monthLabel(key),
      })
    }
    const count = counts.get(key) ?? 0
    const weekday = date.getDay()
    items.push({
      kind: 'day',
      key,
      count,
      dayOfMonth: date.getDate(),
      isWeekend: weekday === 0 || weekday === 6,
      isToday: key === todayKey,
      isMonday: weekday === 1,
      weight: count / max,
    })
    previous = date
  }
  if (fromKey.slice(0, 4) !== toKey.slice(0, 4)) {
    const oldest = items.map(item => item.kind).lastIndexOf('month')
    const month = items[oldest].key
    // The oldest heading is the month of the range's first day.
    items[oldest] = {
      kind: 'month',
      key: month,
      day: fromKey,
      label: `${monthLabel(fromKey)} ${shortYear(dateOfKey(fromKey))}`,
    }
  }
  return items
}

/**
 * The day index for the days that have events, `counts` by day: from the newest such day back to
 * the oldest, with the empty days between them, so the rail spans the list rather than the whole
 * collected range. Empty with fewer than two days.
 */
export function dayIndexOf(counts: ReadonlyMap<DayKey, number>, todayKey: DayKey): DayIndexItem[] {
  const days = [...counts.keys()].filter(day => (counts.get(day) ?? 0) > 0).sort()
  if (days.length < 2) return []
  return buildDayIndex(days[0], days[days.length - 1], counts, todayKey)
}

/** The index of the day with events closest to `index`; the newer day wins a tie. -1 if none. */
export function nearestDayWithEntries(days: readonly DayIndexDay[], index: number): number {
  for (let distance = 0; distance < days.length; distance++) {
    if (days[index - distance]?.count) return index - distance
    if (days[index + distance]?.count) return index + distance
  }
  return -1
}

/** Full labels from 17 px per day, sparse labels from 6 px, bars from 3 px, months below. */
export function densityFor(slotPx: number): DayIndexDensity {
  if (slotPx >= 17) return 'full'
  if (slotPx >= 6) return 'sparse'
  return slotPx >= 3 ? 'dense' : 'months'
}

/**
 * How a rail of months labels them at `rowPx` per month: name, year and bar from 22 px; name
 * and bar from 13 px; quarters and years from 7 px; below that, years only. Unlabelled months
 * keep their bar as a tick.
 */
export function monthDensityFor(rowPx: number): MonthDensity {
  if (rowPx >= 22) return 'months'
  if (rowPx >= 13) return 'months-compact'
  return rowPx >= 7 ? 'quarters' : 'years'
}

export function isMonthDensity(density: DayIndexDensity): density is MonthDensity {
  return density !== 'full' && density !== 'sparse' && density !== 'dense'
}

/**
 * The label of a month row at `density`, or null for a tick. January carries the year where the
 * rows are too thin for a line of its own: `’25` beside month names, `2025` among quarters.
 */
export function monthRowLabel(row: DayIndexMonthRow, density: MonthDensity): string | null {
  const month = Number(row.key.slice(5, 7))
  const year = row.key.slice(0, 4)
  switch (density) {
    case 'months':
      return row.label
    case 'months-compact':
      return month === 1 ? `’${year.slice(2)}` : row.label
    case 'quarters':
      if (month === 1) return year
      return month % 3 === 1 ? row.label : null
    case 'years':
      return month === 1 ? year : null
  }
}

/**
 * The months of `items` (newest first) with their days and events. When the range spans years,
 * the newest month of each older year carries the year, so the rail reads `Jan`, `Dec ’25`.
 */
export function monthRows(items: readonly DayIndexItem[]): DayIndexMonthRow[] {
  const rows: { key: string; label: string; days: number; count: number }[] = []
  for (const item of items) {
    if (item.kind === 'month') {
      rows.push({ key: item.key, label: monthLabel(item.day), days: 0, count: 0 })
    } else if (rows.length > 0) {
      const row = rows[rows.length - 1]
      row.days++
      row.count += item.count
    }
  }
  const max = Math.max(1, ...rows.map(row => row.count))
  const newestYear = rows[0]?.key.slice(0, 4)
  return rows.map((row, index) => {
    const year = row.key.slice(0, 4)
    const firstOfYear = index > 0 && rows[index - 1].key.slice(0, 4) !== year
    return {
      ...row,
      year: firstOfYear && year !== newestYear ? `’${year.slice(2)}` : null,
      weight: row.count / max,
    }
  })
}

/** The index of the centre closest to `y` in ascending `centres`. -1 when there are none. */
export function slotAt(centres: readonly number[], y: number): number {
  if (centres.length === 0) return -1
  let low = 0
  let high = centres.length - 1
  while (low < high) {
    const mid = (low + high) >> 1
    if (centres[mid] < y) low = mid + 1
    else high = mid
  }
  if (low > 0 && Math.abs(centres[low - 1] - y) <= Math.abs(centres[low] - y)) low--
  return low
}
