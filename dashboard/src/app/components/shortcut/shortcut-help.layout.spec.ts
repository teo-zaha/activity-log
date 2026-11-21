/// <reference types="@vitest/browser-playwright" />
// The keyboard help in a real browser (`ng test --configuration layout`, Chromium): the help's
// columns and rows that keep their label and keys apart, which only a layout engine can tell.
import { afterEach, describe, expect, it } from 'vitest'
import { cdp, page } from 'vitest/browser'
import type { ShortcutPlatform } from '../../services/shortcut.service'
import { openApp } from '../../testing/app'

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
})
