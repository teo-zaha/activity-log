import type { ActivityEntry, ActivityLog } from '../generated/activity'

export interface ExportedActivityLog {
  meta: ActivityLog['meta'] | null
  entries: ActivityEntry[]
}
