/// <reference types="@vitest/browser-playwright" />
// The Keyboard card's switch in a real browser (`ng test --configuration layout`, Chromium): a
// settings switch's size beside its label, a finger's target on a touch screen, and an off and an
// on state that read against the card in every theme, which only a layout engine can tell.
import { TestBed } from '@angular/core/testing'
import { afterEach, describe, expect, it } from 'vitest'
import { cdp, page } from 'vitest/browser'
import { THEMES, ThemeService } from '../../../../services/theme.service'
import { openApp } from '../../../../testing/app'

/** A control's parts against what is next to them, WCAG 2.2's 1.4.11. */
const NON_TEXT_CONTRAST = 3
/** The least a finger is given, Apple's 44 pt. */
const TOUCH_TARGET = 44

/** A touch screen: the pointer cannot hover (`ui.no-hover`). */
async function touch(enabled: boolean): Promise<void> {
  await cdp().send('Emulation.setTouchEmulationEnabled', { enabled, maxTouchPoints: 5 })
}

/** Two frames: what a change lays out and what observes it has run. */
async function frames(): Promise<void> {
  for (let i = 0; i < 2; i++) await new Promise(resolve => requestAnimationFrame(resolve))
}

/** Waits until no animation or transition that ends in time runs: the dialog's entrance, which
 * scales it, and the switch's own. Spinners and the scroll-driven fades never end. */
async function settled(): Promise<void> {
  await frames()
  await expect
    .poll(() =>
      document
        .getAnimations()
        .filter(
          animation =>
            animation.playState === 'running' &&
            animation.timeline instanceof DocumentTimeline &&
            animation.effect?.getComputedTiming().endTime !== Infinity
        )
    )
    .toEqual([])
}

/** Opens Settings › General and waits for the Keyboard card's switch. */
async function openSwitch(): Promise<HTMLButtonElement> {
  await openApp('/timeline?settings=general')
  await expect
    .poll(() => document.querySelector('app-settings-keyboard-card [role="switch"]'), {
      timeout: 10_000,
    })
    .not.toBeNull()
  await settled()
  const control = document.querySelector<HTMLButtonElement>(
    'app-settings-keyboard-card [role="switch"]'
  )
  if (!control) throw new Error('the card has its switch')
  return control
}

function part(control: HTMLElement, selector: string): DOMRect {
  const element = control.querySelector(selector)
  if (!element) throw new Error(`the switch has ${selector}`)
  return element.getBoundingClientRect()
}

/** Any CSS colour as the browser paints it, `[r, g, b]` from 0 to 255, alpha laid on white. */
function rgb(colour: string): [number, number, number] {
  const context = new OffscreenCanvas(1, 1).getContext('2d')
  if (!context) throw new Error('a canvas paints')
  context.fillStyle = '#ffffff'
  context.fillRect(0, 0, 1, 1)
  context.fillStyle = colour
  context.fillRect(0, 0, 1, 1)
  const [r = 0, g = 0, b = 0] = context.getImageData(0, 0, 1, 1).data
  return [r, g, b]
}

/** WCAG 2's contrast ratio of two CSS colours. */
function contrast(a: string, b: string): number {
  const luminance = (colour: string): number => {
    const [r, g, bl] = rgb(colour).map(channel => {
      const c = channel / 255
      return c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4
    })
    return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (bl ?? 0)
  }
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x)
  return ((light ?? 0) + 0.05) / ((dark ?? 0) + 0.05)
}

/** The colours Material paints the switch in, as it is now. */
function paint(control: HTMLElement): { outline: string; track: string; handle: string } {
  const track = control.querySelector('.mdc-switch__track')
  const handle = control.querySelector('.mdc-switch__handle')
  if (!track || !handle) throw new Error('the switch has its track and handle')
  return {
    // The off track's outline (`::before`) and the on track's fill (`::after`).
    outline: getComputedStyle(track, '::before').borderTopColor,
    track: getComputedStyle(track, '::after').backgroundColor,
    handle: getComputedStyle(handle, '::after').backgroundColor,
  }
}

describe('the Keyboard card’s switch in a browser', () => {
  afterEach(async () => {
    await touch(false)
    localStorage.clear()
  })

  it('is a settings switch’s size, centred beside its label', async () => {
    await page.viewport(1280, 800)
    const control = await openSwitch()
    expect(control.getAttribute('aria-checked')).toBe('true')

    const track = part(control, '.mdc-switch__track')
    expect(track.width).toBeGreaterThanOrEqual(32)
    expect(track.width).toBeLessThanOrEqual(40)
    expect(track.height).toBeGreaterThanOrEqual(18)
    expect(track.height).toBeLessThanOrEqual(22)
    const handle = part(control, '.mdc-switch__handle')
    expect(handle.height).toBeLessThanOrEqual(16)
    // The handle stays inside its track, on its right while on.
    expect(handle.left).toBeGreaterThanOrEqual(track.left + track.width / 2)
    expect(handle.right).toBeLessThanOrEqual(track.right)
    expect(handle.top).toBeGreaterThanOrEqual(track.top)
    expect(handle.bottom).toBeLessThanOrEqual(track.bottom)

    const text = document.querySelector('app-settings-keyboard-card .keyboard-text')
    if (!text) throw new Error('the card has its label')
    const label = text.getBoundingClientRect()
    // Centred on the label's lines, within a pixel's rounding.
    const offCentre = track.top + track.height / 2 - (label.top + label.height / 2)
    expect(Math.abs(offCentre)).toBeLessThanOrEqual(1)
    expect(track.left).toBeGreaterThan(label.right)
  })

  it('gives a finger at least 44 px each way on a touch screen, centred on the track', async () => {
    await page.viewport(390, 844)
    await touch(true)
    const control = await openSwitch()

    const target = part(control, '.mat-mdc-slide-toggle-touch-target')
    expect(target.width).toBeGreaterThanOrEqual(TOUCH_TARGET)
    expect(target.height).toBeGreaterThanOrEqual(TOUCH_TARGET)
    const track = part(control, '.mdc-switch__track')
    expect(target.left + target.width / 2).toBeCloseTo(track.left + track.width / 2, 0)
    expect(target.top + target.height / 2).toBeCloseTo(track.top + track.height / 2, 0)
    // Where the target reaches, a tap lands on the switch.
    const hit = document.elementFromPoint(target.left + 1, target.top + target.height / 2)
    expect(control.contains(hit)).toBe(true)
  })

  it('reads at 3:1 off and on against the card, in every theme', async () => {
    await page.viewport(1280, 800)
    const control = await openSwitch()
    const card = document.querySelector('app-settings-keyboard-card')
    if (!card) throw new Error('the card is shown')
    const themes = TestBed.inject(ThemeService)

    const failures: string[] = []
    const check = (what: string, ratio: number): void => {
      if (ratio < NON_TEXT_CONTRAST) failures.push(`${what}: ${ratio.toFixed(2)}:1`)
    }
    for (const { id } of THEMES) {
      themes.setTheme(id)
      await settled()
      const surface = getComputedStyle(card).backgroundColor
      for (const checked of ['true', 'false']) {
        if (control.getAttribute('aria-checked') !== checked) {
          control.click()
          await expect.poll(() => control.getAttribute('aria-checked')).toBe(checked)
          await settled()
        }
        const { outline, track, handle } = paint(control)
        if (checked === 'true') {
          check(`${id} on: track on the card`, contrast(track, surface))
          check(`${id} on: handle on the track`, contrast(handle, track))
        } else {
          check(`${id} off: outline on the card`, contrast(outline, surface))
          check(`${id} off: handle on the card`, contrast(handle, surface))
        }
      }
    }
    expect(failures).toEqual([])
  })
})
