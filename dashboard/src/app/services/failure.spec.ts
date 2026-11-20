import { describe, expect, it } from 'vitest'
import { BackendFailure } from './backend/backend-failure'
import { failure, failureOf, failureThen, failureWhile, refusedAs } from './failure'

describe('Failure', () => {
  it('keeps what was thrown as the cause, beside the message for people', () => {
    const thrown = new DOMException('Write permission denied.', 'NotAllowedError')

    expect(failureOf(thrown)).toEqual({
      kind: 'other',
      message: 'Write permission denied.',
      cause: thrown,
    })
    expect(failureWhile('Copying the link', thrown)).toEqual({
      kind: 'other',
      message: 'Copying the link failed: Write permission denied.',
      cause: thrown,
    })
  })

  it('names a failure of its own with no cause, and keeps both of two', () => {
    const refused = new Error('Share refused')
    const first = failureWhile('Sharing the link', refused)
    const then = failure('Copying the link failed: no clipboard')

    expect(then).toEqual({ kind: 'other', message: 'Copying the link failed: no clipboard' })
    expect(failureThen(first, then)).toEqual({
      kind: 'other',
      message: 'Sharing the link failed: Share refused; Copying the link failed: no clipboard',
      cause: [refused, undefined],
    })
  })

  it('keeps a backend failure typed, so callers can tell by its kind', () => {
    const refused = new BackendFailure('invalidRequest', 'end must be after start')

    const shown = failureWhile('Export', refused)

    expect(shown).toEqual({
      kind: 'backend',
      message: 'Export failed: end must be after start',
      cause: refused,
    })
    expect(refusedAs(shown, 'invalidRequest')).toBe(true)
    expect(refusedAs(shown, 'busy')).toBe(false)
    // Worded like the backend's refusal, but not one.
    expect(refusedAs(failure('end must be after start'), 'invalidRequest')).toBe(false)
  })
})
