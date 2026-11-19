import { TestBed } from '@angular/core/testing'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { ThemeService } from './theme.service'

/** Answers `prefers-color-scheme: dark` as a device in dark mode would, or not. */
function deviceDark(dark: boolean): () => void {
  const real = window.matchMedia
  window.matchMedia = (query: string): MediaQueryList => {
    const list = real(query)
    return query === '(prefers-color-scheme: dark)'
      ? { ...list, matches: dark, media: query }
      : list
  }
  return () => (window.matchMedia = real)
}

describe('ThemeService', () => {
  let restore = (): void => undefined
  beforeEach(() => localStorage.clear())
  afterEach(() => restore())

  it('restores the stored theme and stores changes', () => {
    localStorage.setItem('theme', 'mocha')
    const themes = TestBed.inject(ThemeService)

    expect(themes.theme()).toBe('mocha')
    themes.setTheme('dim')
    expect(localStorage.getItem('theme')).toBe('dim')
  })

  it('follows the device, dark or light, until a theme is chosen, and stores nothing until then', async () => {
    restore = deviceDark(true)
    const themes = TestBed.inject(ThemeService)
    TestBed.tick()
    await new Promise(resolve => setTimeout(resolve))

    expect(themes.theme()).toBe('dark')
    expect(document.documentElement.dataset['theme']).toBe('dark')
    expect(localStorage.getItem('theme')).toBeNull()

    themes.setTheme('light')
    expect(themes.theme()).toBe('light')
    expect(localStorage.getItem('theme')).toBe('light')
  })

  it('starts light on a device in light mode', async () => {
    restore = deviceDark(false)
    const themes = TestBed.inject(ThemeService)
    TestBed.tick()
    await new Promise(resolve => setTimeout(resolve))

    expect(themes.theme()).toBe('light')
    expect(localStorage.getItem('theme')).toBeNull()
  })

  it('follows the device for an unknown stored theme', async () => {
    restore = deviceDark(true)
    localStorage.setItem('theme', 'neon')
    const themes = TestBed.inject(ThemeService)
    TestBed.tick()
    await new Promise(resolve => setTimeout(resolve))

    expect(themes.theme()).toBe('dark')
  })
})
