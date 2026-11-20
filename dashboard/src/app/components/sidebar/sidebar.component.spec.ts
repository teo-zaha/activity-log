import { signal } from '@angular/core'
import { provideLocationMocks } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { provideRouter, Router } from '@angular/router'
import { render, screen, within } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { ConnectionKey } from '../../providers/connection'
import { provideAppRouter } from '../../app.routes'
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
    recheckable: true,
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
  it('lists unavailable providers with their reason, opens their settings and checks them again', async () => {
    const open = vi.fn()
    const toggleConnection = vi.fn()
    const recheckConnection = vi.fn(() => Promise.resolve())
    await render(SidebarComponent, {
      providers: [
        {
          provide: ActivityService,
          useValue: {
            selectedDay: signal('2026-11-17'),
            providers: signal([git]),
            unusableProviders: signal([outlook]),
            activeConnections: signal(new Set<ConnectionKey>(['git/default'])),
            toggleConnection,
            isolateConnection: vi.fn(),
            recheckConnection,
          },
        },
        { provide: SettingsDialogLauncher, useValue: { open, pageOpen: signal(false) } },
        provideRouter([]),
      ],
    })

    expect(
      screen.getByText('Startup check timed out after 15s', { selector: '.issue-reason' })
    ).toBeTruthy()

    const gitChip = screen.getByRole('button', { name: /^Git, 4 events/ })
    expect(gitChip.getAttribute('aria-pressed')).toBe('true')
    await userEvent.click(gitChip)
    expect(toggleConnection).toHaveBeenCalledWith('git/default')

    await userEvent.click(screen.getByRole('button', { name: /^Outlook/ }))
    expect(open).toHaveBeenCalledWith('outlook/default')

    await userEvent.click(screen.getByRole('button', { name: 'Check Outlook again' }))
    expect(recheckConnection).toHaveBeenCalledWith(outlook.info.connection)
    expect(open).toHaveBeenCalledTimes(1)
  })

  it('offers to check a source again only when the backend says a check can fix it', async () => {
    await render(SidebarComponent, {
      providers: [
        {
          provide: ActivityService,
          useValue: {
            selectedDay: signal('2026-11-17'),
            providers: signal([git]),
            unusableProviders: signal([
              {
                ...outlook,
                info: { ...outlook.info, problem: 'notConfigured', recheckable: false },
              },
            ]),
            activeConnections: signal(new Set<ConnectionKey>(['git/default'])),
            toggleConnection: vi.fn(),
            isolateConnection: vi.fn(),
            recheckConnection: vi.fn(),
          },
        },
        { provide: SettingsDialogLauncher, useValue: { open: vi.fn(), pageOpen: signal(false) } },
        provideRouter([]),
      ],
    })

    expect(screen.queryByRole('button', { name: 'Check Outlook again' })).toBeNull()
  })

  it('shows a spinner instead of the count while a source is still collected', async () => {
    const toggleConnection = vi.fn()
    const { container } = await render(SidebarComponent, {
      providers: [
        {
          provide: ActivityService,
          useValue: {
            selectedDay: signal('2026-11-17'),
            providers: signal([{ ...git, count: 0, collecting: true }]),
            unusableProviders: signal([]),
            activeConnections: signal(new Set<ConnectionKey>(['git/default'])),
            toggleConnection,
            isolateConnection: vi.fn(),
          },
        },
        { provide: SettingsDialogLauncher, useValue: { open: vi.fn(), pageOpen: signal(false) } },
        provideRouter([]),
      ],
    })

    const chip = screen.getByRole('button', { name: 'Git, collecting' })
    expect(container.querySelector('.source-checking')).not.toBeNull()
    expect(container.querySelector('.source-count')).toBeNull()

    // Unlike a source being checked, it can already be switched off.
    await userEvent.click(chip)
    expect(toggleConnection).toHaveBeenCalledWith('git/default')
  })

  it('links the views, keeping the range and the day, and marks the current one', async () => {
    await render(SidebarComponent, {
      providers: [
        provideAppRouter(),
        provideLocationMocks(),
        {
          provide: ActivityService,
          useValue: {
            selectedDay: signal('2026-11-10'),
            providers: signal([git]),
            unusableProviders: signal([]),
            activeConnections: signal(new Set<ConnectionKey>(['git/default'])),
            toggleConnection: vi.fn(),
            isolateConnection: vi.fn(),
          },
        },
        { provide: SettingsDialogLauncher, useValue: { open: vi.fn(), pageOpen: signal(false) } },
      ],
    })
    const router = TestBed.inject(Router)
    await router.navigateByUrl('/week/2026-11-10?q=review&from=2026-11-01&to=2026-11-14')

    const views = screen.getByRole('navigation', { name: 'Views' })
    const link = (name: string): HTMLElement => within(views).getByRole('link', { name })
    await vi.waitFor(() => expect(link('Week').getAttribute('aria-current')).toBe('page'))
    await vi.waitFor(() => {
      expect(link('Timeline').getAttribute('href')).toBe('/timeline?from=2026-11-01&to=2026-11-14')
      expect(link('Day').getAttribute('href')).toBe('/day/2026-11-10?from=2026-11-01&to=2026-11-14')
      expect(link('Timeline').getAttribute('aria-current')).toBeNull()
    })

    await userEvent.click(link('Day'))
    await vi.waitFor(() => expect(router.url).toBe('/day/2026-11-10?from=2026-11-01&to=2026-11-14'))
  })
})
