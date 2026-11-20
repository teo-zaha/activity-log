import { describe, expect, it } from 'vitest'
import { SourceOperations } from './source-operations'

function held<T>(): {
  promise: Promise<T>
  resolve: (value: T) => void
  reject: (e: Error) => void
} {
  let resolve = (_: T): void => undefined
  let reject = (_: Error): void => undefined
  const promise = new Promise<T>((res, rej) => ((resolve = res), (reject = rej)))
  return { promise, resolve, reject }
}

describe('SourceOperations', () => {
  it('lets only the latest run of a slot change it, whichever ends first', async () => {
    const operations = new SourceOperations()
    const first = held<string>()
    const second = held<string>()
    const firstRun = operations.run('jira/default', 'save', 'Saving failed.', () => first.promise)
    const secondRun = operations.run('jira/default', 'save', 'Saving failed.', () => second.promise)

    second.reject(new Error('Base URL: not a URL'))
    await secondRun
    first.resolve('saved')
    await firstRun

    expect(operations.failure('jira/default', 'save')?.message).toBe('Base URL: not a URL')
    expect(operations.isRunning('jira/default', 'save')).toBe(false)
  })

  it('keeps an earlier run running in the slot of another kind or connection', async () => {
    const operations = new SourceOperations()
    const save = held<void>()
    void operations.run('jira/default', 'save', 'Saving failed.', () => save.promise)

    await operations.run('jira/default', 'recheck', 'Checking failed.', () => Promise.resolve())
    await operations.run('git/default', 'save', 'Saving failed.', () => Promise.resolve())

    expect(operations.isRunning('jira/default', 'save')).toBe(true)
    save.resolve()
  })
})
