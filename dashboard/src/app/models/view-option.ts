import type { ViewMode } from '../services/types'

export interface ViewOption {
  readonly id: ViewMode
  readonly label: string
  readonly shortLabel: string
  readonly icon: string
}

export const VIEW_OPTIONS: readonly ViewOption[] = [
  { id: 'list', label: 'Timeline', shortLabel: 'Timeline', icon: 'view_agenda' },
  { id: 'day', label: 'Day', shortLabel: 'Day', icon: 'calendar_view_day' },
  { id: 'workweek', label: 'Work week', shortLabel: 'Work wk', icon: 'view_week' },
  { id: 'week', label: 'Week', shortLabel: 'Week', icon: 'calendar_view_week' },
]
