import { signal } from '@angular/core'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ActivityService } from '../../services/activity.service'
import type { CollectionStatus } from '../../services/types'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { CollectionStatusComponent } from './collection-status.component'

async function setup(status: CollectionStatus): Promise<{
  retryCollection: ReturnType<typeof vi.fn>
  dismissCollectionStatus: ReturnType<typeof vi.fn>
  open: ReturnType<typeof vi.fn>
}> {
  const retryCollection = vi.fn(() => Promise.resolve())
  const dismissCollectionStatus = vi.fn()
  const open = vi.fn()
  const activity = {
    collectionStatus: signal(status),
    providers: signal([{ label: 'Git' }, { label: 'System' }]),
    unusableProviders: signal([{ info: { id: 'outlook' } }]),
    retryCollection,
    dismissCollectionStatus,
  }

  await render(CollectionStatusComponent, {
    providers: [
      { provide: ActivityService, useValue: activity },
      { provide: SettingsDialogLauncher, useValue: { open } },
    ],
  })

  return { retryCollection, dismissCollectionStatus, open }
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
    expect(open).toHaveBeenCalledWith('outlook')
    expect(dismissCollectionStatus).toHaveBeenCalled()
  })

  it('shows failures without the source shortcut', async () => {
    await setup({ kind: 'failed', operation: 'initial', message: 'offline' })

    const alert = screen.getByRole('alert').textContent
    expect(alert).toContain('Could not load activity')
    expect(alert).toContain('offline')
    expect(screen.queryByRole('button', { name: 'Check sources' })).toBeNull()
  })
})
