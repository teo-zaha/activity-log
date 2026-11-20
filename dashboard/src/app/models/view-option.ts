import type { ViewMode } from '../utils/app-url'

export interface ViewOption {
  readonly id: ViewMode
  readonly label: string
  readonly shortLabel: string
  readonly icon: string
}

export const VIEW_LABELS: Readonly<Record<ViewMode, string>> = {
  list: 'Timeline',
  day: 'Day',
  workweek: 'Work week',
  week: 'Week',
}

export const VIEW_OPTIONS: readonly ViewOption[] = [
  { id: 'list', label: VIEW_LABELS.list, shortLabel: 'Timeline', icon: 'view_agenda' },
  { id: 'day', label: VIEW_LABELS.day, shortLabel: 'Day', icon: 'calendar_view_day' },
  { id: 'workweek', label: VIEW_LABELS.workweek, shortLabel: 'Work wk', icon: 'view_week' },
  { id: 'week', label: VIEW_LABELS.week, shortLabel: 'Week', icon: 'calendar_view_week' },
]
