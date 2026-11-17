import { HttpErrorResponse } from '@angular/common/http'
import type { ErrorResponse } from '../../generated/activity'

/** The backend's message for a failed call, or `fallback` when it gave none. */
export function extractBackendErrorMessage(error: unknown, fallback = 'Unknown error'): string {
  if (error instanceof HttpErrorResponse) {
    const body: unknown = error.error
    if (
      typeof body === 'object' &&
      body !== null &&
      'error' in body &&
      typeof (body as ErrorResponse).error === 'string'
    ) {
      return (body as ErrorResponse).error
    }
    if (typeof body === 'string') return body
    return error.statusText || error.message
  }
  if (error instanceof Error && error.message.trim()) return error.message
  if (typeof error === 'string' && error.trim()) return error
  return fallback
}
