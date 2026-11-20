import { extractBackendErrorMessage } from './backend/error-message'

/**
 * Why an operation failed: a message for people and, where something was thrown, that error
 * itself for diagnostics. Code tells failures apart by where they are held, never by the text.
 * Made only by the constructors below, so a typed kind can join them there.
 */
export interface Failure {
  readonly message: string
  /** What was thrown, kept as it was; logged where the failure is shown. */
  readonly cause?: unknown
}

/** A failure the dashboard itself names, with nothing thrown behind it. */
export function failure(message: string): Failure {
  return { message }
}

/** `error` as a failure: its message, and the error itself as the cause. */
export function failureOf(error: unknown, fallback?: string): Failure {
  // A browser API's refusal (the clipboard, sharing) is a `DOMException`, not always an `Error`.
  const message =
    error instanceof DOMException && error.message
      ? error.message
      : extractBackendErrorMessage(error, fallback)
  return { message, cause: error }
}

/** `error` as the failure of `action`, such as "Export": "Export failed: …". */
export function failureWhile(action: string, error: unknown): Failure {
  return { message: `${action} failed: ${failureOf(error).message}`, cause: error }
}

/** Two failures of one attempt, the first having led to the second, both reasons kept. */
export function failureThen(first: Failure, then: Failure): Failure {
  return { message: `${first.message}; ${then.message}`, cause: [first.cause, then.cause] }
}

/** A failure of an action that has no place of its own to show it, by the action it belongs to. */
export interface Notice {
  readonly origin: 'export' | 'view' | 'settings' | 'recheck'
  readonly failure: Failure
}

/**
 * A failure thrown where only an `Error` can go, such as a resource loader: its `failure` is the
 * one made, cause and all.
 */
export class FailureError extends Error {
  constructor(readonly failure: Failure) {
    super(failure.message, { cause: failure.cause })
  }
}
