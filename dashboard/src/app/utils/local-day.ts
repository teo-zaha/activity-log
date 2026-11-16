// Calendar days in the browser's time zone. The backend only deals in instants; every
// `YYYY-MM-DD` key in the dashboard is a local day computed here.

/** `YYYY-MM-DD` for the given date in the local time zone. */
export function localDateKey(date: Date): string {
  const y = date.getFullYear()
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

/** The local day an instant (RFC 3339 string or epoch milliseconds) falls on. */
export function dayKeyOf(instant: string | number): string {
  return localDateKey(new Date(instant))
}

export function todayKey(): string {
  return localDateKey(new Date())
}

/** Local noon of a `YYYY-MM-DD` key: safe for date arithmetic and labels across DST changes. */
export function dateOfKey(key: string): Date {
  const [y, m, d] = key.split('-').map(Number)
  return new Date(y, m - 1, d, 12)
}

export function addDaysToKey(key: string, days: number): string {
  const date = dateOfKey(key)
  date.setDate(date.getDate() + days)
  return localDateKey(date)
}

/** The instants `[start, end)` covering the local days `fromKey` to `toKey`, inclusive. */
export function localDayRange(fromKey: string, toKey: string): { start: string; end: string } {
  const startOf = (key: string): string => {
    const date = dateOfKey(key)
    date.setHours(0, 0, 0, 0)
    return date.toISOString()
  }
  return { start: startOf(fromKey), end: startOf(addDaysToKey(toKey, 1)) }
}

/** The local day holding the last instant before `end`, an exclusive range end. */
export function lastDayKeyBefore(end: string): string {
  return dayKeyOf(Date.parse(end) - 1)
}

/** "Today" / "Yesterday" for `YYYY-MM-DD` keys relative to `todayKey`, otherwise null. */
export function relativeDayLabel(dateKey: string, todayKey: string): string | null {
  if (dateKey === todayKey) return 'Today'
  return dateKey === addDaysToKey(todayKey, -1) ? 'Yesterday' : null
}
