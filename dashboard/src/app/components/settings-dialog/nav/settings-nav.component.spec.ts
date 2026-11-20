import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { SettingsProviderRow } from '../settings-dialog.types'
import { SettingsNavComponent } from './settings-nav.component'

const gitlab: SettingsProviderRow = {
  info: {
    connection: { source: 'gitlab', name: 'gitlab' },
    label: 'GitLab',
    status: 'degraded',
    usable: true,
    supportedOs: ['macos'],
    reason: 'Token expires soon',
    checkedAt: '2026-11-17T09:00:00Z',
  },
  key: 'gitlab/gitlab',
  label: 'GitLab',
  kindLabel: 'GitLab',
  tooltip: null,
}

describe('SettingsNavComponent', () => {
  it('marks the current section and emits the chosen one', async () => {
    const selected = vi.fn()
    await render(SettingsNavComponent, {
      inputs: { selection: 'logs', providers: [gitlab] },
      on: { selected },
    })

    expect(screen.getByRole('navigation', { name: 'Settings sections' })).toBeTruthy()
    expect(screen.getByRole('button', { name: /Logs/ }).getAttribute('aria-current')).toBe('page')
    expect(
      screen.getByRole('button', { name: /Appearance/ }).getAttribute('aria-current')
    ).toBeNull()

    await userEvent.click(screen.getByRole('button', { name: /GitLab/ }))
    await userEvent.click(screen.getByRole('button', { name: /Add source/ }))

    expect(selected.mock.calls).toEqual([['gitlab/gitlab'], ['add']])
  })

  it('is the settings home on a phone, a tab: rows that open their section and no close button', async () => {
    const selected = vi.fn()
    await render(SettingsNavComponent, {
      inputs: { selection: 'general', providers: [gitlab], home: true },
      on: { selected },
    })

    expect(screen.getByRole('heading', { name: 'Settings' })).toBeTruthy()
    // No section is open beside the list.
    expect(
      screen.getByRole('button', { name: /Appearance/ }).getAttribute('aria-current')
    ).toBeNull()
    expect(screen.getByRole('button', { name: /GitLab/ }).textContent).toContain(
      'Token expires soon'
    )

    await userEvent.click(screen.getByRole('button', { name: /Appearance/ }))
    expect(selected).toHaveBeenCalledWith('general')
    expect(screen.queryByRole('button', { name: 'Close settings' })).toBeNull()
  })
})
