import { HttpErrorResponse } from '@angular/common/http'
import type { ErrorResponse } from '../../generated/activity'

export function extractBackendErrorMessage(error: unknown): string {
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
  if (error instanceof Error) return error.message
  return 'Unknown error'
}
