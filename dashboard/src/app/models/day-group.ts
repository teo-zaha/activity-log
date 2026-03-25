import type { ActivityEntry } from '../generated/activity'

export interface DayGroup {
  date: string
  entries: ActivityEntry[]
}
