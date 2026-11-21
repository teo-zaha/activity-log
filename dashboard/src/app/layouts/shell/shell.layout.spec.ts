/// <reference types="@vitest/browser-playwright" />
// The shell in a real browser (`ng test --configuration layout`, Chromium): what only a layout
// engine can tell, such as controls that overlap, text sizes a touch screen gets, a notice that
// covers a card, a slide that runs. Sizes are the phones' and windows' that broke before.
import { Component } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { MatFormFieldModule } from '@angular/material/form-field'
import { MatInputModule } from '@angular/material/input'
import { afterEach, describe, expect, it } from 'vitest'
import { cdp, page } from 'vitest/browser'
import { failure } from '../../services/failure'
import { NoticeService } from '../../services/notice.service'
import { activityLog, MockBackendTransport } from '../../testing/activity-fixtures'
import { openApp } from '../../testing/app'

const RANGE = 'from=2026-05-17&to=2026-05-18'

/** A field as the settings draw theirs. */
@Component({
  imports: [MatFormFieldModule, MatInputModule],
  template: `<mat-form-field appearance="outline"
    ><input matInput aria-label="Probe"
  /></mat-form-field>`,
})
class MaterialField {}

/** A touch screen: the pointer cannot hover (`ui.no-hover`, `ui.field-text`). */
async function touch(enabled: boolean): Promise<void> {
  await cdp().send('Emulation.setTouchEmulationEnabled', { enabled, maxTouchPoints: 5 })
}

/** The screen's safe area at the sides, as a phone on its side has it (`env()` is 0 here). */
function sideInsets(px: number): () => void {
  const style = document.createElement('style')
  style.textContent = `app-shell-layout { padding-inline: ${px}px !important; }`
  document.head.append(style)
  return () => style.remove()
}

/** Two frames: what a change lays out and what observes it has run. */
async function frames(): Promise<void> {
  for (let i = 0; i < 2; i++) await new Promise(resolve => requestAnimationFrame(resolve))
}

interface Box {
  readonly name: string
  readonly rect: DOMRect
}

/** The header's controls on screen: hidden ones (`display: none`, visually hidden) are left out. */
function headerControls(): Box[] {
  const header = document.querySelector('app-header')
  if (!header) throw new Error('the header is on screen')
  const named: [string, string][] = [
    ['.brand-mark', 'brand'],
    ['.brand-title', 'brand title'],
    ['.range-label', 'range label'],
    ['.range-input', 'date'],
    ['.range-toggle', 'calendar'],
    ['.range-sep', 'arrow'],
    ['.btn-collect', 'Collect'],
    ['.header-actions button', 'action'],
  ]
  return named
    .flatMap(([selector, name]) =>
      [...header.querySelectorAll(selector)].map((element, index) => ({
        name: `${name} ${index + 1}`,
        rect: element.getBoundingClientRect(),
      }))
    )
    .filter(({ rect }) => rect.width > 1 && rect.height > 1)
}

const overlaps = (a: DOMRect, b: DOMRect): boolean =>
  a.left < b.right - 0.5 &&
  b.left < a.right - 0.5 &&
  a.top < b.bottom - 0.5 &&
  b.top < a.bottom - 0.5

/** Pairs of header controls that overlap, and controls cut by the window's edges. */
function collisions(): string[] {
  const boxes = headerControls()
  const found: string[] = []
  boxes.forEach((a, i) =>
    boxes.slice(i + 1).forEach(b => {
      if (overlaps(a.rect, b.rect)) found.push(`${a.name} × ${b.name}`)
    })
  )
  for (const { name, rect } of boxes) {
    if (rect.left < -0.5 || rect.right > innerWidth + 0.5) found.push(`${name} off screen`)
  }
  for (const input of document.querySelectorAll<HTMLInputElement>('.range-input')) {
    if (input.scrollWidth > input.clientWidth + 1) found.push(`${input.value} cut`)
  }
  return found
}

describe('the shell in a browser', () => {
  let undo: (() => void)[] = []
  afterEach(async () => {
    undo.forEach(step => step())
    undo = []
    await touch(false)
  })

  describe('the header', () => {
    const sizes: { name: string; width: number; insets: number; touch: boolean }[] = [
      { name: 'a narrow phone', width: 320, insets: 0, touch: true },
      { name: 'a phone', width: 402, insets: 0, touch: true },
      { name: 'a phone on its side', width: 874, insets: 62, touch: true },
      { name: 'a small window beside insets', width: 641, insets: 62, touch: false },
      { name: 'a window with the range labels', width: 960, insets: 0, touch: false },
      { name: 'a desktop window', width: 1280, insets: 0, touch: false },
    ]
    for (const size of sizes) {
      it(`keeps every control whole and apart on ${size.name}`, async () => {
        await page.viewport(size.width, 800)
        await touch(size.touch)
        undo.push(sideInsets(size.insets))
        await openApp(`/timeline?${RANGE}`)
        await frames()

        expect(collisions()).toEqual([])
      })
    }
  })

  /** Every field people type in, whatever its class. */
  const fieldSizes = (): string[] =>
    [...document.querySelectorAll('input, textarea, select')]
      .filter(field => !(field instanceof HTMLInputElement) || TEXT_INPUTS.has(field.type))
      .map(field => getComputedStyle(field).fontSize)
  const TEXT_INPUTS = new Set(['text', 'search', 'url', 'email', 'password', 'number', 'tel'])

  it('gives every field text a touch screen does not zoom into', async () => {
    await page.viewport(402, 800)
    await touch(true)
    await openApp(`/timeline?${RANGE}`)
    await frames()
    const appFields = fieldSizes()
    // The settings' fields are Material's, which the desktop app's source pane shows.
    const fixture = TestBed.createComponent(MaterialField)
    fixture.detectChanges()
    await frames()

    const sizes = [...appFields, ...fieldSizes()]
    expect(sizes.length).toBeGreaterThanOrEqual(4)
    expect(sizes.filter(size => Number.parseFloat(size) < 16)).toEqual([])
  })

  it('keeps the fields’ own text size where the pointer hovers', async () => {
    await page.viewport(1280, 800)
    await openApp(`/timeline?${RANGE}`)
    await frames()

    expect(fieldSizes()).toEqual(['14px', '14px', '14.5px'])
  })

  it('keeps a notice above the collection’s card, also when the card grows', async () => {
    await page.viewport(402, 800)
    const backend = new MockBackendTransport()
    const failed = {
      start: activityLog.meta.start,
      end: activityLog.meta.end,
      error: 'GitLab unreachable: connection refused',
    }
    backend.activityResponses = [failed]
    backend.activityResponse = failed
    await openApp(`/timeline?${RANGE}`, { backend })
    TestBed.inject(NoticeService).show({
      origin: 'export',
      failure: failure('Export failed: the backend did not answer'),
    })
    await frames()
    const card = document.querySelector<HTMLElement>('app-collection-status .toast')
    const notice = (): DOMRect | undefined =>
      document.querySelector('.mat-mdc-snack-bar-container')?.getBoundingClientRect()
    expect(card).not.toBeNull()
    await expect.poll(() => notice()?.height ?? 0).toBeGreaterThan(0)
    expect(notice()!.bottom).toBeLessThanOrEqual(card!.getBoundingClientRect().top)

    // The card's text wraps onto more lines without anything it says changing (a font coming in).
    card!.style.letterSpacing = '0.3em'
    await frames()
    await expect
      .poll(() => card!.getBoundingClientRect().top - notice()!.bottom)
      .toBeGreaterThanOrEqual(0)
  })

  it('slides the details in on a phone', async () => {
    await page.viewport(402, 800)
    const { router } = await openApp(`/timeline?${RANGE}`)
    const [entry] = activityLog.entries
    const event = encodeURIComponent(`${entry?.source}/default/${entry?.id}`)

    await router.navigateByUrl(`/timeline?${RANGE}&event=${event}`)
    await frames()

    const flyout = document.querySelector('app-event-detail .flyout')
    expect(flyout).not.toBeNull()
    const slides = flyout!
      .getAnimations()
      .map(animation => (animation instanceof CSSAnimation ? animation.animationName : null))
    expect(slides).toEqual(['motion-page-from-right'])
  })

  it('leaves room below the line for the descenders of a button’s label', async () => {
    await page.viewport(1280, 800)
    await openApp(`/timeline?${RANGE}`)
    const label = document.createElement('span')
    label.textContent = 'gypsy'
    Object.assign(label.style, { overflow: 'hidden', textOverflow: 'ellipsis', minWidth: '0' })
    const button = document.querySelector('.btn-collect')
    button?.append(label)
    await frames()

    const style = getComputedStyle(label)
    const context = document.createElement('canvas').getContext('2d')!
    context.font = `${style.fontWeight} ${style.fontSize} ${style.fontFamily}`
    const metrics = context.measureText('gypsy')
    const box = label.getBoundingClientRect()
    const content = metrics.fontBoundingBoxAscent + metrics.fontBoundingBoxDescent
    // The line box centres the font's own box; below its baseline the label keeps this much.
    const belowBaseline = (box.height - content) / 2 + metrics.fontBoundingBoxDescent
    expect(belowBaseline).toBeGreaterThanOrEqual(metrics.actualBoundingBoxDescent)
  })
})
