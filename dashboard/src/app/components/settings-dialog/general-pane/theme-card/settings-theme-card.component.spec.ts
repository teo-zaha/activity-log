import { TestBed } from '@angular/core/testing'
import { createEvent, fireEvent, render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { beforeEach, describe, expect, it } from 'vitest'
import { ThemeService } from '../../../../services/theme.service'
import { SettingsThemeCardComponent } from './settings-theme-card.component'

describe('SettingsThemeCardComponent', () => {
  beforeEach(() => localStorage.clear())

  it('is a radio group of the themes with only the chosen one in the tab order', async () => {
    await render(SettingsThemeCardComponent)

    expect(screen.getByRole('radiogroup', { name: 'Theme' })).toBeTruthy()
    const light = screen.getByRole('radio', { name: 'Light' })
    expect(light.getAttribute('aria-checked')).toBe('true')
    expect(light.tabIndex).toBe(0)
    expect(screen.getByRole('radio', { name: 'Mocha' }).tabIndex).toBe(-1)

    await userEvent.click(screen.getByRole('radio', { name: 'Mocha' }))
    expect(TestBed.inject(ThemeService).theme()).toBe('mocha')
  })

  it('moves the choice and focus with the arrow keys, wrapping at the ends', async () => {
    await render(SettingsThemeCardComponent)
    const themes = TestBed.inject(ThemeService)

    fireEvent.keyDown(screen.getByRole('radiogroup'), { key: 'ArrowRight' })
    expect(themes.theme()).toBe('green')
    expect(document.activeElement).toBe(screen.getByRole('radio', { name: 'Green' }))

    fireEvent.keyDown(screen.getByRole('radiogroup'), { key: 'ArrowLeft' })
    fireEvent.keyDown(screen.getByRole('radiogroup'), { key: 'ArrowLeft' })
    expect(themes.theme()).toBe('midnight')
  })

  it('leaves Alt+← (Back) to the app and the browser, keeping the theme', async () => {
    await render(SettingsThemeCardComponent)
    const group = screen.getByRole('radiogroup')

    const event = createEvent.keyDown(group, { key: 'ArrowLeft', altKey: true })
    fireEvent(group, event)

    expect(event.defaultPrevented).toBe(false)
    expect(TestBed.inject(ThemeService).theme()).toBe('light')
  })
})
