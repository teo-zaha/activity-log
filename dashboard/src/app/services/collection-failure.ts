import { refusedAs } from './failure'
import type { CollectionStatus } from './types'

export type FailedCollection = Extract<CollectionStatus, { readonly kind: 'failed' }>

/** A collect the backend refused as asked: the range, not the backend, is what to change. */
export function refusedRange(status: FailedCollection): boolean {
  return status.origin === 'collect' && refusedAs(status.failure, 'invalidRequest')
}

/** Whether asking again could end otherwise. */
export function retryable(status: FailedCollection): boolean {
  return !refusedRange(status) && status.origin !== 'refusedRead'
}
