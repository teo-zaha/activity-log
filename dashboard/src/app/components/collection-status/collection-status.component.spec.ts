import { signal } from '@angular/core'
import type { ComponentFixture } from '@angular/core/testing'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ActivityService } from '../../services/activity.service'
import { BackendFailure } from '../../services/backend/backend-failure'
import { failure, failureOf } from '../../services/failure'
import type { SourceLink } from '../../services/activity-data.service'
import type { CollectionStatus } from '../../services/types'
import { CollectionStatusComponent } from './collection-status.component'

async function setup(
  status: CollectionStatus,
  collecting: readonly string[] = [],
  link: SourceLink = { status: { kind: 'open' }, since: 0, episode: 0 }
): Promise<{
  retryCollection: ReturnType<typeof vi.fn>
  dismissCollectionStatus: ReturnType<typeof vi.fn>
  fixture: ComponentFixture<CollectionStatusComponent>
}> {
  const retryCollection = vi.fn(() => Promise.resolve())
  const dismissCollectionStatus = vi.fn()
  const activity = {
    collectionStatus: signal(status),
    providers: signal(
      ['Git', 'System'].map(label => ({ label, collecting: collecting.includes(label) }))
    ),
    retryCollection,
    dismissCollectionStatus,
    connection: signal(link),
  }

  const { fixture } = await render(CollectionStatusComponent, {
    providers: [{ provide: ActivityService, useValue: activity }],
  })

  return { retryCollection, dismissCollectionStatus, fixture }
}

describe('CollectionStatusComponent', () => {
  it('renders nothing while idle', async () => {
    await setup({ kind: 'idle' })

    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByText(/collecting/i)).toBeNull()
  })

  it('announces progress with the sources being collected', async () => {
    await setup({ kind: 'collecting', operation: 'collect', startedAt: Date.now() })

    const status = screen.getByRole('status')
    expect(status.textContent).toContain('Collecting activity')
    expect(status.textContent).toContain('From Git, System')
  })

  it('names the sources it still waits for once some are in', async () => {
    await setup({ kind: 'collecting', operation: 'collect', startedAt: Date.now() }, ['System'])

    const status = screen.getByRole('status')
    expect(status.textContent).toContain('Collecting activity')
    expect(status.textContent).toContain('Still collecting System')
    expect(status.textContent).not.toContain('Git')
  })

  it('shows a reconnect over the state, and says when it is out of date', async () => {
    vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] })
    try {
      vi.setSystemTime(10_000)
      const reconnecting: SourceLink = {
        status: { kind: 'reconnecting', failure: failure('connection reset') },
        since: 10_000,
        episode: 0,
      }
      const { fixture } = await setup({ kind: 'idle' }, [], reconnecting)
      expect(screen.getByRole('status').textContent).toContain('Reconnecting')
      expect(screen.getByRole('status').textContent).not.toContain('out of date')

      vi.advanceTimersByTime(5000)
      await fixture.whenStable()
      expect(screen.getByRole('status').textContent).toContain('may be out of date')
    } finally {
      vi.useRealTimers()
    }
  })

  it('shows a lost backend with Retry and Dismiss', async () => {
    const { retryCollection, dismissCollectionStatus } = await setup({
      kind: 'failed',
      operation: 'initial',
      origin: 'lostBackend',
      failure: failure('offline'),
    })

    const alert = screen.getByRole('alert').textContent
    expect(alert).toContain('Could not load activity')
    expect(alert).toContain('offline')
    await userEvent.click(screen.getByRole('button', { name: /retry/i }))
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }))
    expect(retryCollection).toHaveBeenCalled()
    expect(dismissCollectionStatus).toHaveBeenCalled()
  })

  it('offers no retry for a request the backend refused as invalid', async () => {
    const refused = new BackendFailure('invalidRequest', 'A range can span at most 10 years')
    await setup({
      kind: 'failed',
      operation: 'collect',
      origin: 'collect',
      failure: failureOf(refused),
    })

    expect(screen.getByRole('alert').textContent).toContain('at most 10 years')
    expect(screen.queryByRole('button', { name: /retry/i })).toBeNull()
    expect(screen.getByRole('button', { name: 'Dismiss' })).toBeTruthy()
  })

  it('offers retry for a request that may work when sent again', async () => {
    const busy = new BackendFailure('internal', 'The collection could not start')
    await setup({
      kind: 'failed',
      operation: 'collect',
      origin: 'collect',
      failure: failureOf(busy),
    })

    expect(screen.getByRole('button', { name: /retry/i })).toBeTruthy()
  })

  it('offers no retry for a state read the backend refuses for good', async () => {
    const missing = new BackendFailure('notFound', '404 Not Found')
    await setup({
      kind: 'failed',
      operation: 'initial',
      origin: 'refusedRead',
      failure: failureOf(missing),
    })

    expect(screen.getByRole('alert').textContent).toContain('404 Not Found')
    expect(screen.queryByRole('button', { name: /retry/i })).toBeNull()
  })
})
