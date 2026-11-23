/// <reference types="@vitest/browser-playwright" />
// The key tags in a real browser (`ng test --configuration layout`, Chromium): shown while Shift
// is held, they lie over their controls, so nothing moves or changes size, and none is cut by
// what holds it or lies over another control. Only a layout engine can tell.
import { afterEach, describe, expect, it } from 'vitest'
import { page } from 'vitest/browser'
import { createEntryAdapter } from '../../providers/provider-adapter'
import { activityLog } from '../../testing/activity-fixtures'
import { openApp } from '../../testing/app'

const RANGE = 'from=2026-05-17&to=2026-05-18'

const [git, jira] = activityLog.entries.map(entry => createEntryAdapter(entry))
const key = (entry: { trackKey: string } | undefined): string =>
  encodeURIComponent(entry?.trackKey ?? '')
/** The first of a group of two overlapping events: the stepper, the group's back and close. */
const GROUP = `/day/2026-05-18?${RANGE}&events=${key(git)}&events=${key(jira)}&event=${key(git)}`

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

const shownTags = (): { tag: HTMLElement; control: HTMLElement }[] =>
  tags().filter(({ tag }) => getComputedStyle(tag).visibility === 'visible')

/** What can be clicked, typed in or followed, on screen. */
function interactive(): HTMLElement[] {
  return [
    ...document.querySelectorAll<HTMLElement>(
      'button, a[href], input, select, textarea, [role="button"], [role="link"], [role="tab"]'
    ),
  ].filter(element => {
    const rect = element.getBoundingClientRect()
    return rect.width > 1 && rect.height > 1 && getComputedStyle(element).visibility !== 'hidden'
  })
}

/** Where every control is, by its place in the page. */
function controls(): Map<string, string> {
  const boxes = new Map<string, string>()
  interactive().forEach((element, index) => {
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

const nameOf = (element: HTMLElement): string =>
  element.getAttribute('aria-label') ?? `${element.tagName.toLowerCase()}.${element.className}`

/**
 * Each shown tag that a clipping ancestor or the window cuts, or that lies over another control,
 * tagged or not, of the same layer: what an open layer covers (`inert` under it) is behind it.
 */
function misplaced(): string[] {
  const window = new DOMRect(0, 0, innerWidth, innerHeight)
  const others = interactive()
  return shownTags().flatMap(({ tag, control }) => {
    const rect = tag.getBoundingClientRect()
    const name = `${tag.textContent} on ${nameOf(control)}`
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
    const inert = tag.closest('[inert]') !== null
    for (const other of others) {
      const related = other === control || other.contains(control) || control.contains(other)
      if (related || (other.closest('[inert]') !== null) !== inert) continue
      if (overlaps(rect, other.getBoundingClientRect()) && onTopUnder(rect, other)) {
        problems.push(`${name}: over ${nameOf(other)}`)
      }
    }
    return problems
  })
}

/**
 * Whether `other` is what shows where it meets `rect` (a tag, which takes no pointer): not
 * something under a panel, such as the details' backdrop.
 */
function onTopUnder(rect: DOMRect, other: HTMLElement): boolean {
  const box = other.getBoundingClientRect()
  const x = (Math.max(rect.left, box.left) + Math.min(rect.right, box.right)) / 2
  const y = (Math.max(rect.top, box.top) + Math.min(rect.bottom, box.bottom)) / 2
  const hit = document.elementFromPoint(x, y)
  return hit !== null && other.contains(hit)
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

/** Holds Shift until every tag that shows a key here does. */
async function reveal(): Promise<void> {
  shift('keydown')
  // Past Shift at rest, which shows already.
  await expect
    .poll(() => shownTags().filter(({ tag }) => !tag.classList.contains('is-resting')).length, {
      timeout: 3_000,
    })
    .toBeGreaterThan(0)
  await new Promise(resolve => setTimeout(resolve, 200))
  await frames()
}

describe('the key tags in a browser', () => {
  afterEach(() => shift('keyup'))

  const pages: { name: string; url: string; ready: string; tagged: readonly string[] }[] = [
    {
      name: 'the Work week view',
      url: `/workweek/2026-05-18?${RANGE}`,
      ready: '.today-btn',
      tagged: ['Collect activity', 'Previous week', 'Next week'],
    },
    {
      name: 'the Day view',
      url: `/day/2026-05-18?${RANGE}`,
      ready: '.today-btn',
      tagged: ['Collect activity', 'Previous day', 'Next day'],
    },
    {
      name: 'a group’s event in the details',
      url: GROUP,
      ready: 'app-event-detail .flyout-stepper',
      tagged: ['Next event', 'Close event details'],
    },
    {
      name: 'the Timeline',
      url: `/timeline?${RANGE}`,
      ready: 'app-search-bar app-key-tag',
      tagged: ['Filter activity', 'Collect activity'],
    },
  ]
  const sizes = [
    { name: 'a desktop window', width: 1440, height: 960 },
    { name: 'the rail', width: 880, height: 800 },
    { name: 'a phone', width: 402, height: 800 },
  ]

  for (const size of sizes) {
    for (const shown of pages) {
      it(`shows them over their controls, moving and cutting nothing: ${shown.name}, ${size.name}`, async () => {
        await page.viewport(size.width, size.height)
        await openApp(shown.url)
        await expect
          .poll(() => document.querySelector(shown.ready), { timeout: 10_000 })
          .not.toBeNull()
        const before = await settledControls()

        await reveal()

        const tagged = shownTags().map(
          ({ control }) =>
            control.getAttribute('aria-label') ??
            control.querySelector('input')?.getAttribute('aria-label')
        )
        for (const name of shown.tagged) expect(tagged).toContain(name)
        expect(controls()).toEqual(before)
        expect(misplaced()).toEqual([])
      })
    }
  }
})
