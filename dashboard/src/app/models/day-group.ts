import type { DayKey } from '../utils/local-day'

export interface DayGroup<T> {
  date: DayKey
  entries: T[]
}
