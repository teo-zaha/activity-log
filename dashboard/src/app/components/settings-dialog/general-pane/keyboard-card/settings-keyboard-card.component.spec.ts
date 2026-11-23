import { signal } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { BreakpointState } from '../../../../services/breakpoint-state.service'
import { ShortcutPreferences } from '../../../../services/shortcut-preferences.service'
import { SHORTCUT_HELP_DIALOG_ID, SHORTCUT_PLATFORM } from '../../../../services/shortcut.service'
import { SettingsKeyboardCardComponent } from './settings-keyboard-card.component'

const BROWSER = { provide: SHORTCUT_PLATFORM, useValue: { os: 'mac', build: 'browser' } }
const PHONE = { provide: BreakpointState, useValue: { phone: signal(true) } }

describe('SettingsKeyboardCardComponent', () => {
  afterEach(() => localStorage.clear())

  it('switches single-key shortcuts off and on, kept on the device', async () => {
    await render(SettingsKeyboardCardComponent, { providers: [BROWSER] })
    const toggle = screen.getByRole('switch', { name: 'Single-key shortcuts' })
    expect(toggle.getAttribute('aria-checked')).toBe('true')
    expect(toggle.getAttribute('aria-describedby')).toContain('single-key-shortcuts-description')

    await userEvent.click(toggle)
    expect(TestBed.inject(ShortcutPreferences).characterKeys()).toBe('off')
    expect(localStorage.getItem('character-shortcuts')).toBe('off')
    TestBed.resetTestingModule()

    // The next start reads it back.
    await render(SettingsKeyboardCardComponent, { providers: [BROWSER] })
    const again = screen.getByRole('switch', { name: 'Single-key shortcuts' })
    expect(again.getAttribute('aria-checked')).toBe('false')
    await userEvent.click(again)
    expect(TestBed.inject(ShortcutPreferences).characterKeys()).toBe('on')
  })

  it('follows a change another tab makes', async () => {
    const { fixture } = await render(SettingsKeyboardCardComponent, { providers: [BROWSER] })
    localStorage.setItem('character-shortcuts', 'off')
    window.dispatchEvent(
      new StorageEvent('storage', {
        key: 'character-shortcuts',
        newValue: 'off',
        storageArea: localStorage,
      })
    )
    fixture.detectChanges()

    await vi.waitFor(() =>
      expect(
        screen.getByRole('switch', { name: 'Single-key shortcuts' }).getAttribute('aria-checked')
      ).toBe('false')
    )
  })

  it('leaves the help to the sidebar on wider screens', async () => {
    await render(SettingsKeyboardCardComponent, {
      providers: [BROWSER, { provide: BreakpointState, useValue: { phone: signal(false) } }],
    })
    expect(screen.getByRole('switch', { name: 'Single-key shortcuts' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: /Keyboard shortcuts/ })).toBeNull()
  })

  it('opens the help on a phone, which has no sidebar, with its `?` while single keys are on', async () => {
    await render(SettingsKeyboardCardComponent, { providers: [BROWSER, PHONE] })
    const button = screen.getByRole('button', { name: /Keyboard shortcuts/ })
    expect(button.querySelector('app-key-tag')?.textContent).toBe('?')

    await userEvent.click(button)
    expect(TestBed.inject(MatDialog).getDialogById(SHORTCUT_HELP_DIALOG_ID)).toBeDefined()
    // A second click leaves it open: the row opens the help, it does not toggle it.
    await userEvent.click(button)
    expect(TestBed.inject(MatDialog).openDialogs).toHaveLength(1)

    TestBed.inject(ShortcutPreferences).setCharacterKeys('off')
    await vi.waitFor(() =>
      expect(button.querySelector('app-key-tag')?.hasAttribute('hidden')).toBe(true)
    )
  })
})
