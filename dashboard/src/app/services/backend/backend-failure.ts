import { HttpErrorResponse } from '@angular/common/http'
import {
  BACKEND_ERROR_KINDS,
  type BackendError,
  type BackendErrorKind,
} from '../../generated/activity'

/**
 * How a backend call failed: the backend's own kinds, and two only the dashboard can tell. It
 * could not reach the backend (`unreachable`), or this runtime has no such operation
 * (`unsupported`: a desktop command in the browser, an export download in the desktop app).
 * `internal` also covers the runtime's own refusal on the way (a browser API that throws).
 */
export type BackendFailureKind = BackendErrorKind | 'unreachable' | 'unsupported'

/**
 * The one error both transports reject with. Callers decide on `kind`; `message` is for people,
 * and `cause` keeps what the transport got for diagnostics.
 */
export class BackendFailure extends Error {
  override readonly name = 'BackendFailure'

  constructor(
    readonly kind: BackendFailureKind,
    message: string,
    options?: { readonly cause?: unknown }
  ) {
    super(message, options)
  }
}

/**
 * A failed HTTP call: no answer at all, a `BackendError` body, or anything else the server (or a
 * proxy in between) sent, whose body is never shown.
 */
export function httpFailureOf(error: unknown): BackendFailure {
  if (!(error instanceof HttpErrorResponse)) {
    return new BackendFailure('internal', 'The request to the backend failed', { cause: error })
  }
  if (error.status === 0) {
    return new BackendFailure('unreachable', 'The backend cannot be reached', { cause: error })
  }
  const body: unknown = error.error
  if (isBackendError(body)) return new BackendFailure(body.kind, body.error, { cause: error })
  return new BackendFailure('internal', `The backend answered ${error.status} without an error`, {
    cause: error,
  })
}

/**
 * A rejected Tauri command: a command's `BackendError`, or Tauri's own refusal (an unknown
 * command, arguments it cannot pass), which is a plain string.
 */
export function tauriFailureOf(error: unknown): BackendFailure {
  if (isBackendError(error)) return new BackendFailure(error.kind, error.error, { cause: error })
  const message =
    typeof error === 'string' && error.trim() ? error : 'The desktop app failed to run the command'
  return new BackendFailure('internal', message, { cause: error })
}

/** An operation the runtime the dashboard runs in does not offer. */
export function unsupportedIn(
  runtime: 'the browser' | 'desktop mode',
  operation: string
): BackendFailure {
  return new BackendFailure('unsupported', `${operation} is only available in ${runtime}.`)
}

function isBackendError(value: unknown): value is BackendError {
  if (typeof value !== 'object' || value === null) return false
  if (!('kind' in value) || !('error' in value)) return false
  const { kind, error } = value
  return isBackendErrorKind(kind) && typeof error === 'string'
}

function isBackendErrorKind(value: unknown): value is BackendErrorKind {
  return BACKEND_ERROR_KINDS.some(kind => kind === value)
}
