import { signal } from '@angular/core'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { Source } from '../../generated/activity'
import { ActivityService } from '../../services/activity.service'
import type { ProviderViewModel } from '../../services/types'
import { providerInfo } from '../../testing/activity-fixtures'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { SidebarComponent } from './sidebar.component'

const git: ProviderViewModel = {
  info: providerInfo[0],
  label: 'Git',
  icon: 'commit',
  chipKey: 'git',
  tooltip: null,
  count: 4,
}

const outlook: ProviderViewModel = {
  info: {
    id: 'outlook',
    status: 'unavailable',
    usable: false,
    reason: 'Startup check timed out after 15s',
    supportedOs: ['macos', 'windows'],
    checkedAt: '2026-05-18T08:00:00Z',
  },
  label: 'Outlook',
  icon: 'event',
  chipKey: 'outlook',
  tooltip: 'Startup check timed out after 15s',
  count: 0,
}

describe('SidebarComponent', () => {
  it('lists unavailable providers with their reason and opens their settings', async () => {
    const open = vi.fn()
    const toggleSource = vi.fn()
    await render(SidebarComponent, {
      providers: [
        {
          provide: ActivityService,
          useValue: {
            currentView: signal('list'),
            providers: signal([git]),
            unusableProviders: signal([outlook]),
            activeSources: signal(new Set<Source>(['git'])),
            setView: vi.fn(),
            toggleSource,
            isolateSource: vi.fn(),
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
    expect(toggleSource).toHaveBeenCalledWith('git')

    await userEvent.click(screen.getByRole('button', { name: /Outlook/ }))
    expect(open).toHaveBeenCalledWith('outlook')
  })
})
