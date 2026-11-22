import { PRIMARY_OUTLET, type Params, type UrlTree } from '@angular/router'
import {
  SETTINGS_PAGES,
  type SettingsSelection,
} from '../components/settings-dialog/settings-dialog.types'
import { type EntryKey, parseConnectionKey, parseEntryKey } from '../providers/connection'
import { COLLECT_MAX_DAYS } from '../generated/activity'
import { type DayKey, isDayKey, localDayRange } from './local-day'

// The dashboard's URLs: `/timeline` and `/day|workweek|week/YYYY-MM-DD` for the views, and the
// query parameters below. `app.routes.ts` matches them; this module reads and builds them.

/** The views with a local day in their path, declared one by one. */
export const CALENDAR_VIEWS = ['day', 'workweek', 'week'] as const
export type CalendarView = (typeof CALENDAR_VIEWS)[number]
/** The views without a day in their path. */
export const UNDATED_VIEWS = ['list'] as const
export type UndatedView = (typeof UNDATED_VIEWS)[number]
export const VIEW_MODES = [...UNDATED_VIEWS, ...CALENDAR_VIEWS] as const
export type ViewMode = UndatedView | CalendarView

/** The first path segment of each view; the calendar views take a local day after it. */
export const VIEW_PATHS: Readonly<Record<ViewMode, string>> = {
  list: 'timeline',
  day: 'day',
  workweek: 'workweek',
  week: 'week',
}

export function isCalendarView(view: ViewMode): view is CalendarView {
  return CALENDAR_VIEWS.some(calendar => calendar === view)
}

/** What the query parameters hold; a URL's other parameters are ignored. */
export interface AppQuery {
  /** `q`: the timeline's filter text. */
  readonly q: string
  /** `from`/`to`: the collection range, local day keys. */
  readonly from: DayKey | null
  readonly to: DayKey | null
  /** `settings`: the section the settings dialog shows over the view. */
  readonly settings: SettingsSelection | null
  /** `event`: the event whose details show over the view; with `events`, one of them. */
  readonly event: EntryKey | null
  /** `events`, repeated: overlapping events listed together; never just one. */
  readonly events: readonly EntryKey[]
}

/** A dashboard URL: a view, a calendar view's day, and the query. */
export type AppLocation =
  | (AppQuery & { readonly view: UndatedView })
  | (AppQuery & {
      readonly view: CalendarView
      /** Null for a missing or invalid day. */
      readonly day: DayKey | null
    })

/** The day a location's path names; the timeline names none. */
export function dayOf(location: AppLocation): DayKey | null {
  return location.view === 'list' ? null : location.day
}

/** Whether two locations show the same view on the same day, whatever lies over it. */
export function sameView(a: AppLocation, b: AppLocation): boolean {
  return a.view === b.view && dayOf(a) === dayOf(b)
}

/** Query parameters as navigations set them: `null` removes one. */
export interface AppQueryParams {
  readonly q?: string | null
  readonly from?: DayKey | null
  readonly to?: DayKey | null
  readonly settings?: SettingsSelection | null
  readonly event?: EntryKey | null
  readonly events?: readonly EntryKey[] | null
}

export const DEFAULT_LOCATION: AppLocation = {
  view: 'list',
  q: '',
  from: null,
  to: null,
  settings: null,
  event: null,
  events: [],
}

/** Router commands for `view`, on `day` for a calendar view. */
export function viewCommands(view: ViewMode, day: DayKey): string[] {
  return isCalendarView(view) ? [`/${VIEW_PATHS[view]}`, day] : [`/${VIEW_PATHS[view]}`]
}

/** Reads a dashboard URL; null for a path that is no view. `/` is the timeline. */
export function parseAppUrl(url: UrlTree): AppLocation | null {
  const outlets = Object.keys(url.root.children)
  if (outlets.some(outlet => outlet !== PRIMARY_OUTLET)) return null
  const [first, second, ...rest] = (url.root.children[PRIMARY_OUTLET]?.segments ?? []).map(
    segment => segment.path
  )
  const view = first === undefined ? 'list' : viewAt(first)
  if (view === null || rest.length > 0) return null
  const query = queryOf(url.queryParams)
  if (!isCalendarView(view)) return second === undefined ? { view, ...query } : null
  const day = second !== undefined && isDayKey(second) ? second : null
  return { view, day, ...query }
}

/** The query parameters spelling `query`. */
export function queryParamsOf(query: AppQuery): AppQueryParams {
  return {
    q: query.q || null,
    from: query.from,
    to: query.to,
    settings: query.settings,
    event: query.event,
    events: query.events.length > 0 ? query.events : null,
  }
}

/** `home`, `general`, `logs`, `add`, or a connection key such as `gitlab/oss`. */
export function parseSettingsParam(value: string | null): SettingsSelection | null {
  if (value === null) return null
  return SETTINGS_PAGES.find(page => page === value) ?? parseConnectionKey(value)
}

/**
 * Whether the backend would collect the local days `from` to `to`, inclusive, checked as it
 * checks the instants the dashboard sends (`CollectOptions::check`): the end after the start and
 * at most `COLLECT_MAX_DAYS` days apart, so a range across a change to summer time counts its
 * real length. Without both days nothing is collected, which passes.
 */
export function collectableRange(from: string | null, to: string | null): boolean {
  if (from === null || to === null) return true
  if (!isDayKey(from) || !isDayKey(to)) return false
  const { start, end } = localDayRange(from, to)
  const length = Date.parse(end) - Date.parse(start)
  return length > 0 && length <= COLLECT_MAX_DAYS * 86_400_000
}

const DEEP_LINK_SCHEME = 'activity-log:'

/**
 * The app path a deep link names: `activity-log://day/2026-11-17?q=x` is
 * `/day/2026-11-17?q=x`. A plain path is taken as it is; anything else is null.
 */
export function deepLinkPath(link: string): string | null {
  const value = link.trim()
  if (value.startsWith('/')) return value
  if (!value.toLowerCase().startsWith(DEEP_LINK_SCHEME)) return null
  return `/${value.slice(DEEP_LINK_SCHEME.length).replace(/^\/+/, '')}`
}

function viewAt(path: string): ViewMode | null {
  return VIEW_MODES.find(view => VIEW_PATHS[view] === path) ?? null
}

function queryOf(params: Params): AppQuery {
  return {
    q: stringParam(params['q']) ?? '',
    ...rangeOf(dayParam(params['from']), dayParam(params['to'])),
    settings: parseSettingsParam(stringParam(params['settings'])),
    ...detailOf(params['event'], params['events']),
  }
}

/**
 * The details named by `event` and the repeated `events`. A group of fewer than two is its one
 * event; an `event` outside its group keeps the event and drops the group.
 */
function detailOf(eventParam: unknown, eventsParam: unknown): Pick<AppQuery, 'event' | 'events'> {
  const raw = typeof eventsParam === 'string' ? [eventsParam] : eventsParam
  const listed = Array.isArray(raw)
    ? raw.filter((value): value is string => typeof value === 'string')
    : []
  const events = [...new Set(listed.map(parseEntryKey).filter(key => key !== null))]
  const event = parseEntryKey(stringParam(eventParam) ?? '')
  if (events.length < 2) return { event: event ?? events[0] ?? null, events: [] }
  if (event !== null && !events.includes(event)) return { event, events: [] }
  return { event, events }
}

/** A parameter given once; a repeated one arrives as an array and is ignored. */
function stringParam(value: unknown): string | null {
  return typeof value === 'string' ? value : null
}

/**
 * The URL's range, or no range where the backend would refuse it: every use of the range
 * (display, filters, export, the startup collection, deep links) reads it here.
 */
function rangeOf(from: DayKey | null, to: DayKey | null): Pick<AppQuery, 'from' | 'to'> {
  return collectableRange(from, to) ? { from, to } : { from: null, to: null }
}

function dayParam(value: unknown): DayKey | null {
  const day = stringParam(value)
  return day !== null && isDayKey(day) ? day : null
}
