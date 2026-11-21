// Calendar days in the browser's time zone. The backend only deals in instants; every
// `YYYY-MM-DD` key in the dashboard is a local day computed here.

/**
 * A local day as `YYYY-MM-DD`. The type only names the shape and checks nothing: `'1-2-3'` and
 * `'2026-13-45'` have it too. What holds a real day is made by `localDateKey` (and the functions
 * built on it) or let through by `isDayKey`; text from outside (a URL, a link) goes through
 * `isDayKey`.
 */
export type DayKey = `${number}-${number}-${number}`

/** `YYYY-MM-DD` for the given date in the local time zone. */
export function localDateKey(date: Date): DayKey {
  const y = date.getFullYear()
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  // Padded digits around dashes: the shape `DayKey` names, which the template cannot infer.
  return `${y}-${m}-${d}` as DayKey
}

/** The local day an instant (RFC 3339 string or epoch milliseconds) falls on. */
export function dayKeyOf(instant: string | number): DayKey {
  return localDateKey(new Date(instant))
}

export function todayKey(): DayKey {
  return localDateKey(new Date())
}

/** Where an instant shows on a wall clock: its day and the minutes since that day's midnight. */
export interface WallClock {
  readonly day: DayKey
  /** `hours * 60 + minutes` as the clock reads, seconds dropped: 23 or 25 real hours fit in 24. */
  readonly minutes: number
}

const ZONED_CLOCKS = new Map<string, Intl.DateTimeFormat>()

function zonedClock(timeZone: string): Intl.DateTimeFormat {
  let format = ZONED_CLOCKS.get(timeZone)
  if (!format) {
    format = new Intl.DateTimeFormat('en-GB', {
      timeZone,
      hourCycle: 'h23',
      year: 'numeric',
      month: '2-digit',
      day: '2-digit',
      hour: '2-digit',
      minute: '2-digit',
    })
    ZONED_CLOCKS.set(timeZone, format)
  }
  return format
}

/**
 * The wall clock at an instant (epoch milliseconds), in `timeZone` (an IANA name) or, without
 * one, in the browser's zone as every other local day is read.
 */
export function wallClock(instant: number, timeZone?: string): WallClock {
  if (timeZone === undefined) {
    const date = new Date(instant)
    return { day: localDateKey(date), minutes: date.getHours() * 60 + date.getMinutes() }
  }
  const part = (parts: Intl.DateTimeFormatPart[], type: Intl.DateTimeFormatPartTypes): number =>
    Number(parts.find(p => p.type === type)?.value)
  const parts = zonedClock(timeZone).formatToParts(instant)
  const date = new Date(part(parts, 'year'), part(parts, 'month') - 1, part(parts, 'day'), 12)
  return { day: localDateKey(date), minutes: part(parts, 'hour') * 60 + part(parts, 'minute') }
}

/** Whether `value` is a `YYYY-MM-DD` key of a real calendar day, such as one read from a URL. */
export function isDayKey(value: string): value is DayKey {
  if (!/^\d{4}-\d{2}-\d{2}$/.test(value)) return false
  const [y, m, d] = value.split('-').map(Number)
  return localDateKey(new Date(y, m - 1, d, 12)) === value
}

/** Local noon of a `YYYY-MM-DD` key: safe for date arithmetic and labels across DST changes. */
export function dateOfKey(key: DayKey): Date {
  const [y, m, d] = key.split('-').map(Number)
  return new Date(y, m - 1, d, 12)
}

export function addDaysToKey(key: DayKey, days: number): DayKey {
  const date = dateOfKey(key)
  date.setDate(date.getDate() + days)
  return localDateKey(date)
}

/** The instants `[start, end)` covering the local days `fromKey` to `toKey`, inclusive. */
export function localDayRange(fromKey: DayKey, toKey: DayKey): { start: string; end: string } {
  const startOf = (key: DayKey): string => {
    const date = dateOfKey(key)
    date.setHours(0, 0, 0, 0)
    return date.toISOString()
  }
  return { start: startOf(fromKey), end: startOf(addDaysToKey(toKey, 1)) }
}

/** The keys within `fromKey`–`toKey`, inclusive; an empty bound does not limit. */
export function keysWithin<Key extends string>(
  keys: readonly Key[],
  fromKey: DayKey | null,
  toKey: DayKey | null
): Key[] {
  return keys.filter(key => (!fromKey || key >= fromKey) && (!toKey || key <= toKey))
}

/** The local day holding the last instant before `end`, an exclusive range end. */
export function lastDayKeyBefore(end: string): DayKey {
  return dayKeyOf(Date.parse(end) - 1)
}

/** "Today" / "Yesterday" for `YYYY-MM-DD` keys relative to `todayKey`, otherwise null. */
export function relativeDayLabel(dateKey: DayKey, todayKey: DayKey): string | null {
  if (dateKey === todayKey) return 'Today'
  return dateKey === addDaysToKey(todayKey, -1) ? 'Yesterday' : null
}

// Built once: creating a formatter per call (as `toLocaleTimeString` with options does) is slow
// enough to show when a list formats every entry.
const CLOCK_TIME = new Intl.DateTimeFormat('en-GB', { hour: '2-digit', minute: '2-digit' })
const LONG_DAY = new Intl.DateTimeFormat('en-GB', {
  weekday: 'long',
  year: 'numeric',
  month: 'long',
  day: 'numeric',
})

/** `14:05`: the local time of an instant. */
export function clockTime(instant: string): string {
  return CLOCK_TIME.format(new Date(instant))
}

/** `Monday, 16 November 2026` for a `YYYY-MM-DD` key. */
export function longDayLabel(key: DayKey): string {
  return LONG_DAY.format(dateOfKey(key))
}

const DAY_MONTH = new Intl.DateTimeFormat('en-GB', { day: 'numeric', month: 'short' })
const DAY_MONTH_YEAR = new Intl.DateTimeFormat('en-GB', {
  day: 'numeric',
  month: 'short',
  year: 'numeric',
})

/** `1 Apr – 8 Apr 2026`, or `28 Dec 2025 – 3 Jan 2026` across a year: the days of a range. */
export function dayRangeLabel(from: DayKey, to: DayKey): string {
  const [start, end] = [dateOfKey(from), dateOfKey(to)]
  const first = start.getFullYear() === end.getFullYear() ? DAY_MONTH : DAY_MONTH_YEAR
  return `${first.format(start)} – ${DAY_MONTH_YEAR.format(end)}`
}

const MONTH = new Intl.DateTimeFormat('en-GB', { month: 'short' })

/** `Nov`: three letters for every month (en-GB writes September as "Sept"). */
export function monthLabel(key: DayKey): string {
  return MONTH.format(dateOfKey(key)).slice(0, 3)
}
