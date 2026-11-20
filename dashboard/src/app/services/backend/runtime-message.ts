import type {
  ActivityState,
  ActivityUpdate,
  ConnectionCount,
  ConnectionId,
  ProviderInfo,
  ProvidersUpdate,
  RuntimePing,
  RuntimeState,
} from '../../generated/activity'
import { isSource } from '../../providers/connection'
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

/** What the views read of a source before anything else: who it is, and whether it is usable. */
function isProviderInfo(value: unknown): value is ProviderInfo {
  return (
    isRecord(value) &&
    isConnectionId(value['connection']) &&
    typeof value['label'] === 'string' &&
    typeof value['status'] === 'string' &&
    typeof value['usable'] === 'boolean'
  )
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
    isListOf(providers['providers'], isProviderInfo)
  )
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
