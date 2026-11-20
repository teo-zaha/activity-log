import { signal } from '@angular/core'
import { provideLocationMocks } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { render, screen, within } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { provideAppRouter } from '../../app.routes'
import { ActivityService } from '../../services/activity.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { SidebarComponent } from './sidebar.component'

describe('SidebarComponent', () => {
  it('links the views, keeping the range and the day, and marks the current one', async () => {
    await render(SidebarComponent, {
      providers: [
        provideAppRouter(),
        provideLocationMocks(),
        {
          provide: ActivityService,
          useValue: { selectedDay: signal('2026-11-10') },
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

  it('lays itself out as the shared breakpoint state says, as the source list does', async () => {
    const { container } = await render(SidebarComponent, {
      providers: [
        provideAppRouter(),
        provideLocationMocks(),
        { provide: ActivityService, useValue: { selectedDay: signal('2026-11-10') } },
        { provide: SettingsDialogLauncher, useValue: { open: vi.fn(), pageOpen: signal(false) } },
        { provide: BreakpointState, useValue: { phone: signal(false), rail: signal(true) } },
      ],
    })

    expect(container.querySelector('.sidebar')?.classList.contains('is-rail')).toBe(true)
  })
})
