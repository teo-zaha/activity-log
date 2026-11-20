import { describe, expect, it } from 'vitest'
import { failure, failureOf, failureThen, failureWhile } from './failure'

describe('Failure', () => {
  it('keeps what was thrown as the cause, beside the message for people', () => {
    const thrown = new DOMException('Write permission denied.', 'NotAllowedError')

    expect(failureOf(thrown)).toEqual({ message: 'Write permission denied.', cause: thrown })
    expect(failureWhile('Copying the link', thrown)).toEqual({
      message: 'Copying the link failed: Write permission denied.',
      cause: thrown,
    })
  })

  it('names a failure of its own with no cause, and keeps both of two', () => {
    const refused = new Error('Share refused')
    const first = failureWhile('Sharing the link', refused)
    const then = failure('Copying the link failed: no clipboard')

    expect(then).toEqual({ message: 'Copying the link failed: no clipboard' })
    expect(failureThen(first, then)).toEqual({
      message: 'Sharing the link failed: Share refused; Copying the link failed: no clipboard',
      cause: [refused, undefined],
    })
  })
})
