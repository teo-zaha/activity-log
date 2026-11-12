/** `YYYY-MM-DD` for the given date in the local time zone. */
export function localDateKey(date: Date): string {
  const y = date.getFullYear()
  const m = String(date.getMonth() + 1).padStart(2, '0')
  const d = String(date.getDate()).padStart(2, '0')
  return `${y}-${m}-${d}`
}

/** "Today" / "Yesterday" for `YYYY-MM-DD` keys relative to `todayKey`, otherwise null. */
export function relativeDayLabel(dateKey: string, todayKey: string): string | null {
  if (dateKey === todayKey) return 'Today'
  const yesterday = new Date(todayKey + 'T12:00:00')
  yesterday.setDate(yesterday.getDate() - 1)
  return dateKey === localDateKey(yesterday) ? 'Yesterday' : null
}
