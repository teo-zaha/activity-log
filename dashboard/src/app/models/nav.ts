import type { ViewMode } from '../utils/app-url'

/** A destination of the app's navigation: the sidebar, the rail and the phone's bottom bar. */
export interface NavItem {
  readonly id: ViewMode
  readonly label: string
  /** The label under the icon in the phone's bottom bar. */
  readonly shortLabel: string
  readonly icon: string
}

/** The navigation's groups, in order. The views are the only one so far. */
export type NavGroupId = 'views'

export interface NavGroup {
  readonly id: NavGroupId
  readonly label: string
  readonly items: readonly NavItem[]
}

export const VIEW_LABELS: Readonly<Record<ViewMode, string>> = {
  list: 'Timeline',
  day: 'Day',
  workweek: 'Work week',
  week: 'Week',
}

/** The one list the navigation is drawn from. A destination is an item here plus its route. */
export const NAV_GROUPS: readonly NavGroup[] = [
  {
    id: 'views',
    label: 'View',
    items: [
      { id: 'list', label: VIEW_LABELS.list, shortLabel: 'Timeline', icon: 'view_agenda' },
      { id: 'day', label: VIEW_LABELS.day, shortLabel: 'Day', icon: 'calendar_view_day' },
      { id: 'workweek', label: VIEW_LABELS.workweek, shortLabel: 'Work wk', icon: 'view_week' },
      { id: 'week', label: VIEW_LABELS.week, shortLabel: 'Week', icon: 'calendar_view_week' },
    ],
  },
]
