import type {
  ActivityState,
  ActivityUpdate,
  CalendarFeedGuide,
  CalendarFeedService,
  ConnectionCount,
  ConnectionId,
  KeyFileReason,
  LocalApp,
  NoStoreReason,
  OsStoreState,
  ProviderInfo,
  ProviderFixAction,
  ProvidersUpdate,
  RuntimePing,
  RuntimeState,
  SecretStoreInfo,
  SystemSettingsPane,
  WebFixAction,
} from '../../generated/activity'
import { isSource } from '../../providers/connection'
import { isWebLink } from '../../utils/text-links'
import type { RuntimeEventName, RuntimeMessage } from './runtime-stream'

/**
 * Reads what a transport received for `event` as its message, or `null` when it does not have
 * that event's shape: the stamp (epoch, authority, a safe integer revision), the discriminant,
 * the payload a reader takes apart, and a pair's two parts of one epoch and authority. Small on
 * purpose: it checks what the coordinator and the views read first (each source's connection,
 * label, status and usable; a log's range, pending connections and counts), not every entry. Both
 * transports use it, the desktop's handshake read included.
 */
export function decodeRuntimeMessage(
  event: RuntimeEventName,
  payload: unknown
): RuntimeMessage | null {
  switch (event) {
    case 'state':
      return isRuntimeState(payload) ? { event, payload } : null
    case 'activity':
      return isActivityUpdate(payload) ? { event, payload } : null
    case 'providers':
      return isProvidersUpdate(payload) ? { event, payload } : null
  }
}

/** Whether `payload` is the `ping` the event stream sends while nothing changes. */
export function isRuntimePing(payload: unknown): payload is RuntimePing {
  return isRecord(payload) && typeof payload['epoch'] === 'string'
}

/** Every activity kind, so a new one in the generated union does not compile here unread. */
const ACTIVITY_KINDS: Readonly<Record<ActivityState['kind'], true>> = {
  empty: true,
  running: true,
  ready: true,
  failed: true,
}

function isRecord(value: unknown): value is Readonly<Record<string, unknown>> {
  return typeof value === 'object' && value !== null && !Array.isArray(value)
}

function hasStamp(value: Readonly<Record<string, unknown>>): boolean {
  const revision = value['revision']
  return (
    typeof value['epoch'] === 'string' &&
    typeof value['authority'] === 'string' &&
    typeof revision === 'number' &&
    Number.isSafeInteger(revision) &&
    revision >= 0
  )
}

/** A connection of a source this dashboard knows: an unknown one would have no adapter. */
function isConnectionId(value: unknown): value is ConnectionId {
  if (!isRecord(value)) return false
  const source = value['source']
  return typeof source === 'string' && isSource(source) && typeof value['name'] === 'string'
}

/**
 * What the views read of a source before anything else: who it is, whether it is usable and
 * whether it can be checked again.
 */
function isProviderInfo(value: unknown): value is ProviderInfo {
  if (!isRecord(value)) return false
  return (
    isConnectionId(value['connection']) &&
    typeof value['label'] === 'string' &&
    typeof value['status'] === 'string' &&
    typeof value['usable'] === 'boolean' &&
    typeof value['recheckable'] === 'boolean' &&
    (value['fixAction'] === undefined || isProviderFixAction(value['fixAction']))
  )
}

const SYSTEM_SETTINGS_PANES: Readonly<Record<SystemSettingsPane, true>> = {
  automation: true,
  filesAndFolders: true,
}

const LOCAL_APPS: Readonly<Record<LocalApp, true>> = { outlook: true }

function isProviderFixAction(value: unknown): value is ProviderFixAction {
  if (!isRecord(value)) return false
  switch (value['kind']) {
    case 'web':
      return isWebFixAction(value['action'])
    case 'local': {
      const target = value['target']
      if (!isRecord(target)) return false
      switch (target['kind']) {
        case 'systemSettings':
          return isOneOf(target['pane'], SYSTEM_SETTINGS_PANES)
        case 'openApp':
          return isOneOf(target['app'], LOCAL_APPS)
        default:
          return false
      }
    }
    default:
      return false
  }
}

export function isWebFixAction(value: unknown): value is WebFixAction {
  if (!isRecord(value)) return false
  switch (value['kind']) {
    case 'createToken':
      return (
        typeof value['instructions'] === 'string' &&
        isWebUrl(value['url']) &&
        (value['classicUrl'] === undefined || isWebUrl(value['classicUrl']))
      )
    case 'feedHelp':
      return (
        isListOf(value['guides'], isCalendarFeedGuide) &&
        (value['recovery'] === undefined || typeof value['recovery'] === 'string')
      )
    default:
      return false
  }
}

const FEED_SERVICES: Readonly<Record<CalendarFeedService, true>> = {
  outlookWork: true,
  outlookCom: true,
  googleCalendar: true,
  iCloud: true,
  fastmail: true,
}

function isCalendarFeedGuide(value: unknown): value is CalendarFeedGuide {
  return (
    isRecord(value) &&
    isOneOf(value['service'], FEED_SERVICES) &&
    typeof value['label'] === 'string' &&
    typeof value['instructions'] === 'string' &&
    (value['url'] === undefined || isWebUrl(value['url']))
  )
}

function isWebUrl(value: unknown): value is string {
  return typeof value === 'string' && isWebLink(value)
}

function isConnectionCount(value: unknown): value is ConnectionCount {
  return (
    isRecord(value) && isConnectionId(value['connection']) && typeof value['count'] === 'number'
  )
}

function isListOf<T>(value: unknown, item: (value: unknown) => value is T): value is T[] {
  return Array.isArray(value) && value.every(entry => item(entry))
}

/** A log as the coordinator takes it apart: its range, pending connections, counts, entries. */
function isLog(value: unknown): boolean {
  if (!isRecord(value)) return false
  const { meta, summary } = value
  return (
    isRecord(meta) &&
    typeof meta['start'] === 'string' &&
    typeof meta['end'] === 'string' &&
    (meta['pending'] === undefined || isListOf(meta['pending'], isConnectionId)) &&
    isRecord(summary) &&
    isListOf(summary['connections'], isConnectionCount) &&
    Array.isArray(value['entries'])
  )
}

function isActivityState(value: unknown): value is ActivityState {
  if (!isRecord(value)) return false
  const kind = value['kind']
  if (typeof kind !== 'string' || !Object.hasOwn(ACTIVITY_KINDS, kind)) return false
  if (kind === 'running' || kind === 'ready') return isLog(value['log'])
  if (kind === 'failed') {
    const failure = value['failure']
    return (
      isRecord(failure) &&
      typeof failure['start'] === 'string' &&
      typeof failure['end'] === 'string' &&
      typeof failure['error'] === 'string'
    )
  }
  return true
}

function isActivityUpdate(value: unknown): value is ActivityUpdate {
  return isRecord(value) && hasStamp(value) && isActivityState(value['state'])
}

function isProvidersUpdate(value: unknown): value is ProvidersUpdate {
  if (!isRecord(value) || !hasStamp(value)) return false
  const providers = value['providers']
  return (
    isRecord(providers) &&
    typeof providers['currentOs'] === 'string' &&
    isListOf(providers['providers'], isProviderInfo) &&
    isSecretStoreInfo(providers['secretStore'])
  )
}

// Every value of each union, typed by it, so a new one is a compile error until it is read.
const OS_STORE_STATES: Readonly<Record<OsStoreState, true>> = { ready: true, refused: true }
const KEY_FILE_REASONS: Readonly<Record<KeyFileReason, true>> = {
  noOsStore: true,
  forced: true,
  devProfile: true,
}
const NO_STORE_REASONS: Readonly<Record<NoStoreReason, true>> = { noOsStore: true }

function isOneOf<T extends string>(value: unknown, values: Readonly<Record<T, true>>): value is T {
  return typeof value === 'string' && Object.hasOwn(values, value)
}

/** Where new secrets are sealed, which the settings show: one of the store's own states. */
function isSecretStoreInfo(value: unknown): value is SecretStoreInfo {
  if (!isRecord(value)) return false
  switch (value['store']) {
    case 'os':
      return isOneOf(value['state'], OS_STORE_STATES)
    case 'file':
      return isOneOf(value['why'], KEY_FILE_REASONS)
    case 'unavailable':
      return isOneOf(value['why'], NO_STORE_REASONS)
    default:
      return false
  }
}

function isRuntimeState(value: unknown): value is RuntimeState {
  if (!isRecord(value)) return false
  const { activity, providers } = value
  return (
    isActivityUpdate(activity) &&
    isProvidersUpdate(providers) &&
    activity.epoch === providers.epoch &&
    activity.authority === providers.authority
  )
}
