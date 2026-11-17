import { signal } from '@angular/core'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { ConnectionKey } from '../../providers/connection'
import { ActivityService } from '../../services/activity.service'
import type { ProviderViewModel } from '../../services/types'
import { providerInfo } from '../../testing/activity-fixtures'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { SidebarComponent } from './sidebar.component'

const git: ProviderViewModel = {
  info: providerInfo[0],
  key: 'git/default',
  label: 'Git',
  chipKey: 'git',
  tooltip: null,
  count: 4,
  collecting: false,
}

const outlook: ProviderViewModel = {
  key: 'outlook/default',
  info: {
    connection: { source: 'outlook', name: 'default' },
    label: 'Outlook',
    status: 'unavailable',
    usable: false,
    reason: 'Startup check timed out after 15s',
    supportedOs: ['macos', 'windows'],
    checkedAt: '2026-05-18T08:00:00Z',
  },
  label: 'Outlook',
  chipKey: 'outlook',
  tooltip: 'Startup check timed out after 15s',
  count: 0,
  collecting: false,
}

describe('SidebarComponent', () => {
  it('lists unavailable providers with their reason and opens their settings', async () => {
    const open = vi.fn()
    const toggleConnection = vi.fn()
    await render(SidebarComponent, {
      providers: [
        {
          provide: ActivityService,
          useValue: {
            currentView: signal('list'),
            providers: signal([git]),
            unusableProviders: signal([outlook]),
            activeConnections: signal(new Set<ConnectionKey>(['git/default'])),
            setView: vi.fn(),
            toggleConnection,
            isolateConnection: vi.fn(),
          },
        },
        { provide: SettingsDialogLauncher, useValue: { open } },
      ],
    })

    expect(
      screen.getByText('Startup check timed out after 15s', { selector: '.issue-reason' })
    ).toBeTruthy()

    const gitChip = screen.getByRole('button', { name: /^Git, 4 events/ })
    expect(gitChip.getAttribute('aria-pressed')).toBe('true')
    await userEvent.click(gitChip)
    expect(toggleConnection).toHaveBeenCalledWith('git/default')

    await userEvent.click(screen.getByRole('button', { name: /Outlook/ }))
    expect(open).toHaveBeenCalledWith('outlook/default')
  })

  it('shows a spinner instead of the count while a source is still collected', async () => {
    const toggleConnection = vi.fn()
    const { container } = await render(SidebarComponent, {
      providers: [
        {
          provide: ActivityService,
          useValue: {
            currentView: signal('list'),
            providers: signal([{ ...git, count: 0, collecting: true }]),
            unusableProviders: signal([]),
            activeConnections: signal(new Set<ConnectionKey>(['git/default'])),
            setView: vi.fn(),
            toggleConnection,
            isolateConnection: vi.fn(),
          },
        },
        { provide: SettingsDialogLauncher, useValue: { open: vi.fn() } },
      ],
    })

    const chip = screen.getByRole('button', { name: 'Git, collecting' })
    expect(container.querySelector('.source-checking')).not.toBeNull()
    expect(container.querySelector('.source-count')).toBeNull()

    // Unlike a source being checked, it can already be switched off.
    await userEvent.click(chip)
    expect(toggleConnection).toHaveBeenCalledWith('git/default')
  })
})
