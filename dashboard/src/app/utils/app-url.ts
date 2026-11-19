import { PRIMARY_OUTLET, type Params, type UrlTree } from '@angular/router'
import type { SettingsSelection } from '../components/settings-dialog/settings-dialog.types'
import { type EntryKey, parseConnectionKey, parseEntryKey } from '../providers/connection'
import { VIEW_MODES, type ViewMode } from '../services/types'
import { isDayKey } from './local-day'

// The dashboard's URLs: `/timeline` and `/day|workweek|week/YYYY-MM-DD` for the views, and the
// query parameters below. `app.routes.ts` matches them; this module reads and builds them.

/** The first path segment of each view; the calendar views take a local day after it. */
export const VIEW_PATHS: Readonly<Record<ViewMode, string>> = {
  list: 'timeline',
  day: 'day',
  workweek: 'workweek',
  week: 'week',
}

export type CalendarView = Exclude<ViewMode, 'list'>

/** What the query parameters hold; a URL's other parameters are ignored. */
export interface AppQuery {
  /** `q`: the timeline's filter text. */
  readonly q: string
  /** `from`/`to`: the collection range, local day keys. */
  readonly from: string | null
  readonly to: string | null
  /** `settings`: the section the settings dialog shows over the view. */
  readonly settings: SettingsSelection | null
  /** `event`: the event whose details show over the view; with `events`, one of them. */
  readonly event: EntryKey | null
  /** `events`, repeated: overlapping events listed together; never just one. */
  readonly events: readonly EntryKey[]
}

export interface AppLocation extends AppQuery {
  readonly view: ViewMode
  /** The day of a calendar view; null on the timeline, and for a missing or invalid day. */
  readonly day: string | null
}

/** Query parameters as navigations set them: `null` removes one. */
export interface AppQueryParams {
  readonly q?: string | null
  readonly from?: string | null
  readonly to?: string | null
  readonly settings?: string | null
  readonly event?: string | null
  readonly events?: readonly string[] | null
}

export const DEFAULT_LOCATION: AppLocation = {
  view: 'list',
  day: null,
  q: '',
  from: null,
  to: null,
  settings: null,
  event: null,
  events: [],
}

/** Router commands for `view`, on `day` for a calendar view. */
export function viewCommands(view: ViewMode, day: string): string[] {
  return view === 'list' ? [`/${VIEW_PATHS.list}`] : [`/${VIEW_PATHS[view]}`, day]
}

/** Reads a dashboard URL; null for a path that is no view. `/` is the timeline. */
export function parseAppUrl(url: UrlTree): AppLocation | null {
  const outlets = Object.keys(url.root.children)
  if (outlets.some(outlet => outlet !== PRIMARY_OUTLET)) return null
  const [first, second, ...rest] = (url.root.children[PRIMARY_OUTLET]?.segments ?? []).map(
    segment => segment.path
  )
  const view = first === undefined ? 'list' : viewAt(first)
  if (view === null || rest.length > 0 || (view === 'list' && second !== undefined)) return null
  const day = view !== 'list' && second !== undefined && isDayKey(second) ? second : null
  return { view, day, ...queryOf(url.queryParams) }
}

/** The query parameters spelling `query`. */
export function queryParamsOf(query: AppQuery): AppQueryParams {
  return {
    q: query.q || null,
    from: query.from,
    to: query.to,
    settings: query.settings === null ? null : settingsParam(query.settings),
    event: query.event,
    events: query.events.length > 0 ? query.events : null,
  }
}

const CONNECTION_PREFIX = 'connection:'

/** `home`, `general`, `logs`, `add`, or a connection key such as `gitlab/oss`. */
export function settingsParam(selection: SettingsSelection): string {
  return selection.startsWith(CONNECTION_PREFIX)
    ? selection.slice(CONNECTION_PREFIX.length)
    : selection
}

export function parseSettingsParam(value: string | null): SettingsSelection | null {
  if (value === 'home' || value === 'general' || value === 'logs' || value === 'add') return value
  const key = value === null ? null : parseConnectionKey(value)
  return key === null ? null : `${CONNECTION_PREFIX}${key}`
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
    from: dayParam(params['from']),
    to: dayParam(params['to']),
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

function dayParam(value: unknown): string | null {
  const day = stringParam(value)
  return day !== null && isDayKey(day) ? day : null
}
