import type { SettingsSelection } from '../components/settings-dialog/settings-dialog.types'
import { BackendFailure, type BackendFailureKind } from './backend/backend-failure'

/**
 * Why an operation failed: a message for people and, where something was thrown, that error
 * itself for diagnostics. Code tells failures apart by where they are held and, for a backend
 * call, by its `cause.kind`; never by the text. Made only by the constructors below.
 */
export type Failure =
  /** A backend call failed; `cause.kind` says how. */
  | { readonly kind: 'backend'; readonly message: string; readonly cause: BackendFailure }
  /** Anything else: a failure the dashboard names itself, a browser API's refusal. */
  | { readonly kind: 'other'; readonly message: string; readonly cause?: unknown }

/** A failure the dashboard itself names, with nothing thrown behind it. */
export function failure(message: string): Failure {
  return { kind: 'other', message }
}

/** `error` as a failure: its message, and the error itself as the cause. */
export function failureOf(error: unknown, fallback = 'Unknown error'): Failure {
  if (error instanceof BackendFailure) {
    return { kind: 'backend', message: error.message, cause: error }
  }
  return { kind: 'other', message: messageOf(error) ?? fallback, cause: error }
}

/** `error` as the failure of `action`, such as "Export": "Export failed: …". */
export function failureWhile(action: string, error: unknown): Failure {
  const failed = failureOf(error)
  return { ...failed, message: `${action} failed: ${failed.message}` }
}

/** Whether the backend refused the call as `kind`. */
export function refusedAs(failure: Failure, kind: BackendFailureKind): boolean {
  return failure.kind === 'backend' && failure.cause.kind === kind
}

/** What the snackbar shows: a failure, or news that is none (`InfoNotice`). */
export type Notice = FailureNotice | InfoNotice

/**
 * A failure of an action that has no place of its own to show it, by the action it belongs to,
 * and what its button does besides dismissing it.
 */
export interface FailureNotice {
  readonly origin: 'export' | 'view' | 'settings' | 'recheck' | 'import' | 'localFix'
  readonly failure: Failure
  readonly action?: NoticeAction
}

/** News with something to do and nothing failed: 0.0.7's settings imported, values left out. */
export interface InfoNotice {
  readonly origin: 'import'
  readonly info: string
  readonly action: NoticeAction
}

/**
 * A notice's button that does something: it opens the settings at `selection`, or imports
 * 0.0.7's settings with the keychain allowed to ask.
 */
export type NoticeAction =
  | { readonly kind: 'openSettings'; readonly selection: SettingsSelection }
  | { readonly kind: 'importLegacy' }

/**
 * A failure thrown where only an `Error` can go, such as a resource loader: its `failure` is the
 * one made, cause and all.
 */
export class FailureError extends Error {
  constructor(readonly failure: Failure) {
    super(failure.message, { cause: failure.cause })
  }
}

/** The message of a thrown value; a browser API's refusal is a `DOMException`, not always an `Error`. */
function messageOf(error: unknown): string | null {
  if ((error instanceof Error || error instanceof DOMException) && error.message.trim()) {
    return error.message
  }
  if (typeof error === 'string' && error.trim()) return error
  return null
}
