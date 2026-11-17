import type { DayIndexDay, DayIndexDensity, DayIndexItem } from '../models/day-index'
import { addDaysToKey, dateOfKey, monthLabel } from './local-day'

/**
 * Every day from `toKey` back to `fromKey` (newest first, like the timeline), with a month
 * heading before each month's first row. A heading where the year changes shows the year.
 */
export function buildDayIndex(
  fromKey: string,
  toKey: string,
  counts: ReadonlyMap<string, number>,
  todayKey: string
): DayIndexItem[] {
  if (fromKey > toKey) return []
  const max = Math.max(1, ...counts.values())
  const items: DayIndexItem[] = []
  let previous: Date | null = null
  for (let key = toKey; key >= fromKey; key = addDaysToKey(key, -1)) {
    const date = dateOfKey(key)
    if (previous?.getMonth() !== date.getMonth()) {
      const yearChanged = previous !== null && previous.getFullYear() !== date.getFullYear()
      items.push({
        kind: 'month',
        key: key.slice(0, 7),
        label: yearChanged ? `’${String(date.getFullYear()).slice(2)}` : monthLabel(key),
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
  return items
}

/** The index of the day with events closest to `index`; the newer day wins a tie. -1 if none. */
export function nearestDayWithEntries(days: readonly DayIndexDay[], index: number): number {
  for (let distance = 0; distance < days.length; distance++) {
    if (days[index - distance]?.count) return index - distance
    if (days[index + distance]?.count) return index + distance
  }
  return -1
}

/** Full labels from 17 px per day, sparse labels from 6 px, bars below that. */
export function densityFor(slotPx: number): DayIndexDensity {
  if (slotPx >= 17) return 'full'
  return slotPx >= 6 ? 'sparse' : 'dense'
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
