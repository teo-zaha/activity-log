/// <reference types="@vitest/browser-playwright" />
// The keyboard help and the Keyboard card in a real browser (`ng test --configuration layout`,
// Chromium): the help's columns, rows that keep their label and keys apart, and the card on a
// touch screen, which only a layout engine can tell.
import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { afterEach, describe, expect, it } from 'vitest'
import { cdp, page } from 'vitest/browser'
import type { ShortcutPlatform } from '../../services/shortcut.service'
import { openApp } from '../../testing/app'
import { openShortcutHelp } from './shortcut-help-dialog.component'

const BROWSER: ShortcutPlatform = { os: 'mac', build: 'browser' }
const WINDOWS_DESKTOP: ShortcutPlatform = { os: 'other', build: 'desktop' }

/** A touch screen: the pointer cannot hover (`ui.no-hover`). */
async function touch(enabled: boolean): Promise<void> {
  await cdp().send('Emulation.setTouchEmulationEnabled', { enabled, maxTouchPoints: 5 })
}

/** Two frames: what a change lays out and what observes it has run. */
async function frames(): Promise<void> {
  for (let i = 0; i < 2; i++) await new Promise(resolve => requestAnimationFrame(resolve))
}

/** Opens the help as `?` does, and waits until it is laid out. */
async function openHelp(): Promise<HTMLElement> {
  document.body.dispatchEvent(
    new KeyboardEvent('keydown', { key: '?', code: 'Slash', bubbles: true, cancelable: true })
  )
  await expect.poll(() => document.querySelector('app-shortcut-help-dialog')).not.toBeNull()
  await frames()
  const help = document.querySelector<HTMLElement>('app-shortcut-help-dialog')
  if (!help) throw new Error('the help is open')
  return help
}

/** The distinct left edges of the help's sections: one per column. */
function columns(help: HTMLElement): number[] {
  const lefts = [...help.querySelectorAll('.help-section')].map(section =>
    Math.round(section.getBoundingClientRect().left)
  )
  return [...new Set(lefts)]
}

/** Rows whose label and keys overlap, or that are wider than their column. */
function brokenRows(help: HTMLElement): string[] {
  return [...help.querySelectorAll<HTMLElement>('.help-row')].flatMap(row => {
    const label = row.querySelector('.help-label')?.getBoundingClientRect()
    const keys = row.querySelector('.help-keys')?.getBoundingClientRect()
    const name = row.querySelector('.help-label')?.textContent ?? '?'
    if (!label || !keys) return [`${name}: parts missing`]
    const overlap =
      label.left < keys.right - 0.5 &&
      keys.left < label.right - 0.5 &&
      label.top < keys.bottom - 0.5 &&
      keys.top < label.bottom - 0.5
    const wide = row.scrollWidth > row.clientWidth + 1
    return [...(overlap ? [`${name}: label under keys`] : []), ...(wide ? [`${name}: cut`] : [])]
  })
}

describe('the keyboard help in a browser', () => {
  afterEach(async () => {
    await touch(false)
    localStorage.clear()
  })

  it('shows its sections in two columns on a wide screen, every row whole', async () => {
    await page.viewport(1280, 800)
    await openApp('/timeline', { platform: BROWSER })
    const help = await openHelp()

    expect(columns(help)).toHaveLength(2)
    expect(brokenRows(help)).toEqual([])
    expect(help.getBoundingClientRect().bottom).toBeLessThanOrEqual(innerHeight)
  })

  it('keeps one column for one section, as with single keys off, using its width', async () => {
    await page.viewport(1280, 800)
    localStorage.setItem('character-shortcuts', 'off')
    await openApp('/timeline', { platform: { os: 'mac', build: 'desktop' } })
    // From its row in the settings: `?` is off.
    openShortcutHelp(TestBed.inject(MatDialog))
    await expect.poll(() => document.querySelector('app-shortcut-help-dialog')).not.toBeNull()
    await frames()
    const help = document.querySelector<HTMLElement>('app-shortcut-help-dialog')
    if (!help) throw new Error('the help is open')

    expect(help.querySelectorAll('.help-section')).toHaveLength(1)
    const section = help.querySelector('.help-section')?.getBoundingClientRect()
    const sections = help.querySelector('.help-sections')?.getBoundingClientRect()
    expect(section?.width).toBeCloseTo(sections?.width ?? 0, 0)
    expect(brokenRows(help)).toEqual([])
  })

  it('shows one column on a phone, scrolling within the screen, three keys wrapping', async () => {
    await page.viewport(390, 844)
    await openApp('/timeline', { platform: WINDOWS_DESKTOP })
    const help = await openHelp()

    expect(columns(help)).toHaveLength(1)
    expect(brokenRows(help)).toEqual([])
    const body = help.querySelector<HTMLElement>('.help-body')
    if (!body) throw new Error('the help has a body')
    expect(getComputedStyle(body).overflowY).toBe('auto')
    expect(help.getBoundingClientRect().bottom).toBeLessThanOrEqual(innerHeight)
  })

  it('shows the Keyboard card on a touch screen, its `?` tag hidden until Shift is held', async () => {
    await page.viewport(402, 800)
    await touch(true)
    await openApp('/timeline?settings=general', { platform: BROWSER })

    await expect
      .poll(() => document.querySelector('app-settings-keyboard-card'), { timeout: 10_000 })
      .not.toBeNull()
    await frames()
    const card = document.querySelector<HTMLElement>('app-settings-keyboard-card')
    const toggle = card?.querySelector('[role="switch"]')?.getBoundingClientRect()
    expect(toggle?.width ?? 0).toBeGreaterThan(0)
    const tag = card?.querySelector('app-key-tag')
    if (!tag) throw new Error('the help row has its key tag')
    expect(getComputedStyle(tag).visibility).toBe('hidden')
  })
})
