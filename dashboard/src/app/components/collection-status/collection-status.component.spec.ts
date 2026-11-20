import { signal } from '@angular/core'
import type { ComponentFixture } from '@angular/core/testing'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ActivityService } from '../../services/activity.service'
import type { SourceLink } from '../../services/activity-data.service'
import type { CollectionStatus } from '../../services/types'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { CollectionStatusComponent } from './collection-status.component'

async function setup(
  status: CollectionStatus,
  collecting: readonly string[] = [],
  link: SourceLink = { status: { kind: 'open' }, since: 0, episode: 0 }
): Promise<{
  retryCollection: ReturnType<typeof vi.fn>
  dismissCollectionStatus: ReturnType<typeof vi.fn>
  open: ReturnType<typeof vi.fn>
  fixture: ComponentFixture<CollectionStatusComponent>
}> {
  const retryCollection = vi.fn(() => Promise.resolve())
  const dismissCollectionStatus = vi.fn()
  const open = vi.fn()
  const activity = {
    collectionStatus: signal(status),
    providers: signal(
      ['Git', 'System'].map(label => ({ label, collecting: collecting.includes(label) }))
    ),
    unusableProviders: signal([{ key: 'outlook/default' }]),
    retryCollection,
    dismissCollectionStatus,
    connection: signal(link),
  }

  const { fixture } = await render(CollectionStatusComponent, {
    providers: [
      { provide: ActivityService, useValue: activity },
      { provide: SettingsDialogLauncher, useValue: { open } },
    ],
  })

  return { retryCollection, dismissCollectionStatus, open, fixture }
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

  it('offers retry and source checks after a timeout', async () => {
    const { retryCollection, open, dismissCollectionStatus } = await setup({
      kind: 'timedOut',
      operation: 'collect',
      message: 'Timed out waiting for collection.',
    })

    expect(screen.getByRole('alert').textContent).toContain('Collection timed out')

    await userEvent.click(screen.getByRole('button', { name: /retry/i }))
    await userEvent.click(screen.getByRole('button', { name: 'Check sources' }))
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }))

    expect(retryCollection).toHaveBeenCalled()
    expect(open).toHaveBeenCalledWith('outlook/default')
    expect(dismissCollectionStatus).toHaveBeenCalled()
  })

  it('shows a reconnect over the state, and says when it is out of date', async () => {
    vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] })
    try {
      vi.setSystemTime(10_000)
      const reconnecting: SourceLink = {
        status: { kind: 'reconnecting', failure: { message: 'connection reset' } },
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

  it('shows failures without the source shortcut', async () => {
    await setup({ kind: 'failed', operation: 'initial', message: 'offline' })

    const alert = screen.getByRole('alert').textContent
    expect(alert).toContain('Could not load activity')
    expect(alert).toContain('offline')
    expect(screen.queryByRole('button', { name: 'Check sources' })).toBeNull()
  })
})
