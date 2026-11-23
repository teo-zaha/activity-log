/// <reference types="@vitest/browser-playwright" />
// The key tags in a real browser (`ng test --configuration layout`, Chromium): shown while Shift
// is held, they lie over their controls, so nothing moves or changes size, and none is cut by
// what holds it or lies over another control. Only a layout engine can tell.
import { afterEach, describe, expect, it } from 'vitest'
import { page } from 'vitest/browser'
import { openApp } from '../../testing/app'

const RANGE = 'from=2026-05-17&to=2026-05-18'

/** Two frames: what a change lays out and what observes it has run. */
async function frames(): Promise<void> {
  for (let i = 0; i < 2; i++) await new Promise(resolve => requestAnimationFrame(resolve))
}

function shift(type: 'keydown' | 'keyup'): void {
  document.body.dispatchEvent(
    new KeyboardEvent(type, { key: 'Shift', shiftKey: type === 'keydown', bubbles: true })
  )
}

/** The tags that show a key, each with the control it lies over. */
function tags(): { tag: HTMLElement; control: HTMLElement }[] {
  return [...document.querySelectorAll<HTMLElement>('app-key-tag:not([hidden])')].flatMap(tag => {
    const control = tag.parentElement
    return control && control.getBoundingClientRect().width > 0 ? [{ tag, control }] : []
  })
}

/** Where every control of the header, the sidebar and the views is, by its place in the page. */
function controls(): Map<string, string> {
  const boxes = new Map<string, string>()
  document
    .querySelectorAll<HTMLElement>(
      'app-header button, app-sidebar a, app-sidebar button, main button, main input'
    )
    .forEach((element, index) => {
      const rect = element.getBoundingClientRect()
      boxes.set(
        `${index} ${element.tagName.toLowerCase()}.${element.className}`,
        [rect.left, rect.top, rect.width, rect.height].join(' ')
      )
    })
  return boxes
}

const within = (inner: DOMRect, outer: DOMRect): boolean =>
  inner.left >= outer.left - 0.5 &&
  inner.top >= outer.top - 0.5 &&
  inner.right <= outer.right + 0.5 &&
  inner.bottom <= outer.bottom + 0.5

const overlaps = (a: DOMRect, b: DOMRect): boolean =>
  a.left < b.right - 0.5 &&
  b.left < a.right - 0.5 &&
  a.top < b.bottom - 0.5 &&
  b.top < a.bottom - 0.5

/** Each shown tag that a clipping ancestor or the window cuts, or that lies over another control. */
function misplaced(): string[] {
  const shown = tags().filter(({ tag }) => getComputedStyle(tag).visibility === 'visible')
  const window = new DOMRect(0, 0, innerWidth, innerHeight)
  return shown.flatMap(({ tag, control }) => {
    const rect = tag.getBoundingClientRect()
    const name = `${tag.textContent} on ${control.getAttribute('aria-label') ?? control.className}`
    const problems: string[] = []
    if (!within(rect, window)) problems.push(`${name}: cut by the window`)
    for (let box = tag.parentElement; box && box !== document.body; box = box.parentElement) {
      const { display, overflowX, overflowY } = getComputedStyle(box)
      // `display: contents` draws no box of its own, so it cuts nothing.
      const clips =
        display !== 'contents' && [overflowX, overflowY].some(overflow => overflow !== 'visible')
      if (clips && !within(rect, box.getBoundingClientRect())) {
        problems.push(`${name}: cut by ${box.tagName.toLowerCase()}.${box.className}`)
      }
    }
    for (const other of shown) {
      if (other.control !== control && overlaps(rect, other.control.getBoundingClientRect())) {
        problems.push(
          `${name}: over ${other.control.getAttribute('aria-label') ?? other.control.className}`
        )
      }
    }
    return problems
  })
}

/** Where the controls are once the page has stopped moving (fonts, the collection coming in). */
async function settledControls(): Promise<Map<string, string>> {
  await document.fonts.ready
  let last = controls()
  for (;;) {
    await frames()
    await new Promise(resolve => setTimeout(resolve, 100))
    const now = controls()
    if (JSON.stringify([...now]) === JSON.stringify([...last])) return now
    last = now
  }
}

describe('the key tags in a browser', () => {
  afterEach(() => shift('keyup'))

  for (const size of [
    { name: 'a desktop window', width: 1440, height: 960 },
    { name: 'the rail', width: 880, height: 800 },
    { name: 'a phone', width: 402, height: 800 },
  ]) {
    it(`shows them over their controls, moving and cutting nothing, in ${size.name}`, async () => {
      await page.viewport(size.width, size.height)
      await openApp(`/workweek/2026-05-18?${RANGE}`)
      await expect
        .poll(() => document.querySelector('.today-btn'), { timeout: 10_000 })
        .not.toBeNull()
      const before = await settledControls()
      expect(tags().length).toBeGreaterThan(4)

      shift('keydown')
      await expect
        .poll(() => tags().every(({ tag }) => getComputedStyle(tag).visibility === 'visible'))
        .toBe(true)
      await frames()

      expect(controls()).toEqual(before)
      expect(misplaced()).toEqual([])
    })
  }

  it('shows the filter’s tag inside the field, as the Timeline’s other controls keep still', async () => {
    await page.viewport(1440, 960)
    await openApp(`/timeline?${RANGE}`)
    await expect
      .poll(() => document.querySelector('app-search-bar app-key-tag'), { timeout: 10_000 })
      .not.toBeNull()
    const before = await settledControls()

    shift('keydown')
    const tag = document.querySelector<HTMLElement>('app-search-bar app-key-tag')
    await expect.poll(() => tag && getComputedStyle(tag).visibility).toBe('visible')
    await frames()

    expect(controls()).toEqual(before)
    expect(misplaced()).toEqual([])
  })
})
