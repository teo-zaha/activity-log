import { signal } from '@angular/core'
import type { ConnectionKey } from '../../../providers/connection'
import { type Failure, failureOf } from '../../../services/failure'

export type SourceOperation = 'save' | 'remove' | 'recheck'

/** One run of an operation: `id` names it, so only its own completion changes it. */
type Invocation =
  | { readonly id: number; readonly phase: 'running' }
  | { readonly id: number; readonly phase: 'failed'; readonly failure: Failure }

export type OperationResult<T> = { readonly ok: true; readonly value: T } | { readonly ok: false }

/**
 * The saves, removals and checks a pane started, by connection and by kind: a save and a check
 * of one connection run side by side. The latest run of each owns its slot; an earlier one's
 * answer no longer changes it. A failure stays with its slot until that operation runs again.
 */
export class SourceOperations {
  private readonly invocations = signal<ReadonlyMap<string, Invocation>>(new Map())
  private lastId = 0

  isRunning(key: ConnectionKey, operation: SourceOperation): boolean {
    return this.invocations().get(slot(key, operation))?.phase === 'running'
  }

  failure(key: ConnectionKey, operation: SourceOperation): Failure | null {
    const invocation = this.invocations().get(slot(key, operation))
    return invocation?.phase === 'failed' ? invocation.failure : null
  }

  async run<T>(
    key: ConnectionKey,
    operation: SourceOperation,
    fallback: string,
    work: () => Promise<T>
  ): Promise<OperationResult<T>> {
    const id = ++this.lastId
    this.settle(slot(key, operation), null, { id, phase: 'running' })
    try {
      const value = await work()
      this.settle(slot(key, operation), id, null)
      return { ok: true, value }
    } catch (error) {
      const failure = failureOf(error, fallback)
      this.settle(slot(key, operation), id, { id, phase: 'failed', failure })
      return { ok: false }
    }
  }

  /** Sets `name` to `next` if run `owner` still owns it (any run, for `null`). */
  private settle(name: string, owner: number | null, next: Invocation | null): void {
    this.invocations.update(invocations => {
      const current = invocations.get(name)
      if (owner !== null && current?.id !== owner) return invocations
      const updated = new Map(invocations)
      if (next === null) updated.delete(name)
      else updated.set(name, next)
      return updated
    })
  }
}

function slot(key: ConnectionKey, operation: SourceOperation): string {
  return `${operation}:${key}`
}
