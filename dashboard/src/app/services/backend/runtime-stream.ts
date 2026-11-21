import type {
  ActivityUpdate,
  ProvidersUpdate,
  RUNTIME_EVENTS,
  RuntimeState,
} from '../../generated/activity'
import type { BackendFailure } from './backend-failure'

/**
 * The events the host's state comes as, the same on both transports
 * (`docs/plans/shared-contracts.md` §5): `GET /api/events` names its server-sent events so, and
 * the desktop app emits Tauri events of these names.
 */
interface RuntimePayloads {
  /** Both parts of one moment. */
  readonly state: RuntimeState
  readonly activity: ActivityUpdate
  readonly providers: ProvidersUpdate
}

/** Each event's payload; an event the backend adds has none here, which does not compile. */
export type RuntimeEvents = { readonly [Name in RuntimeEventName]: RuntimePayloads[Name] }

export type RuntimeEventName = Exclude<(typeof RUNTIME_EVENTS)[number], 'ping'>

/** One message of the stream, tagged by its event. */
export type RuntimeMessage = {
  readonly [Name in RuntimeEventName]: {
    readonly event: Name
    readonly payload: RuntimeEvents[Name]
  }
}[RuntimeEventName]

/**
 * What one stream delivers, in the order it receives it; never before `openRuntimeStream`
 * returned.
 */
export interface RuntimeStreamHandlers {
  /** The first message of every stream is a `state`; later ones come after an authority change. */
  state(pair: RuntimeState): void
  activity(update: ActivityUpdate): void
  providers(update: ProvidersUpdate): void
  /** The stream ended without `close`: no answer, or a message it could not read. */
  lost(failure: BackendFailure): void
  /**
   * Bytes arrived, before the message they belong to is whole: the stream is alive. A transport
   * that delivers whole messages only (the desktop's) never calls it.
   */
  progress(): void
}

/** One connection to the host's state. After `close` or `lost` it delivers nothing. */
export interface RuntimeStream {
  close(): void
}

/** Hands `message` to the handler of its event. */
export function deliver(handlers: RuntimeStreamHandlers, message: RuntimeMessage): void {
  switch (message.event) {
    case 'state':
      return handlers.state(message.payload)
    case 'activity':
      return handlers.activity(message.payload)
    case 'providers':
      return handlers.providers(message.payload)
  }
}

/** The host a message is of. */
export function epochOf(message: RuntimeMessage): string {
  return message.event === 'state' ? message.payload.activity.epoch : message.payload.epoch
}
