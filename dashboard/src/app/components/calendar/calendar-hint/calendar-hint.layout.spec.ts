/// <reference types="@vitest/browser-playwright" />
// The hint of an empty calendar view in a real browser (`ng test --configuration layout`,
// Chromium): its frosted layer covers the visible day columns and nothing above or beside them
// (the hour labels stay sharp), whatever the grid is scrolled to, and the hint sits in their
// middle, which only a layout engine can tell.
import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { page } from 'vitest/browser'
import { ActivityViewStateService } from '../../../services/activity-view-state.service'
import { openApp } from '../../../testing/app'
import { APP_RENDER_TEST } from '../../../testing/timeouts'
import { DEFAULT_SCROLL_HOUR } from '../calendar-grid'

// The collection on screen holds 17–18 May 2026: these are not collected.
const WEEK = '/week/2026-06-01'
const DAY = '/day/2026-06-03'

type View = 'app-week-view' | 'app-day-view'

/** Two frames: what a change lays out and what observes it has run. */
async function frames(): Promise<void> {
  for (let i = 0; i < 2; i++) await new Promise(resolve => requestAnimationFrame(resolve))
}

function element(selector: string): HTMLElement {
  const found = document.querySelector<HTMLElement>(selector)
  if (!found) throw new Error(`${selector} is on screen`)
  return found
}

async function open(url: string, view: View, width = 1280, height = 800): Promise<HTMLElement> {
  await page.viewport(width, height)
  await openApp(url)
  await expect.poll(() => document.querySelector(`${view} app-calendar-hint`)).not.toBeNull()
  const scroller = element(`${view} .cal-scroll`)
  const start = DEFAULT_SCROLL_HOUR * TestBed.inject(ActivityViewStateService).hourHeight()
  await expect.poll(() => scroller.scrollTop).toBe(start)
  await frames()
  return scroller
}

/**
 * The part of the day columns on screen: from the first column's left edge to the last one's
 * right edge, below the scroller's sticky header.
 */
function visibleColumns(view: View, scroller: HTMLElement): DOMRect {
  const columns = [...document.querySelectorAll(`${view} .day-col`)].map(column =>
    column.getBoundingClientRect()
  )
  if (columns.length === 0) throw new Error(`${view} has day columns`)
  const box = scroller.getBoundingClientRect()
  const header = document.querySelector(`${view} .cal-sticky-header`)?.getBoundingClientRect()
  const top = header ? header.bottom : box.top
  const [left, right] = [columns[0].left, columns[columns.length - 1].right]
  return new DOMRect(left, top, right - left, box.top + scroller.clientHeight - top)
}

/** How far the layer is from the visible day columns, and the hint from their middle, in px. */
function placement(view: View, scroller: HTMLElement): Record<string, number> {
  const grid = visibleColumns(view, scroller)
  const layer = element(`${view} .cal-hint-layer`).getBoundingClientRect()
  const hint = element(`${view} app-calendar-hint`).getBoundingClientRect()
  return {
    top: Math.abs(layer.top - grid.top),
    bottom: Math.abs(layer.bottom - grid.bottom),
    left: Math.abs(layer.left - grid.left),
    right: Math.abs(layer.right - grid.right),
    centreX: Math.abs(hint.left + hint.width / 2 - (grid.left + grid.width / 2)),
    centreY: Math.abs(hint.top + hint.height / 2 - (grid.top + grid.height / 2)),
  }
}

const IN_PLACE = { top: 1, bottom: 1, left: 1, right: 1, centreX: 2, centreY: 2 }

function expectInPlace(view: View, scroller: HTMLElement): void {
  const off = placement(view, scroller)
  for (const [side, most] of Object.entries(IN_PLACE)) {
    expect(off[side], side).toBeLessThanOrEqual(most)
  }
  expectHoursClear(view)
}

/** The hour labels lie beside the layer, never under it: they stay sharp. */
function expectHoursClear(view: View): void {
  const layer = element(`${view} .cal-hint-layer`).getBoundingClientRect()
  const hours = element(`${view} app-calendar-hours`).getBoundingClientRect()
  expect(layer.left).toBeGreaterThanOrEqual(hours.right - 0.5)
  const label = document.elementFromPoint(
    hours.left + hours.width / 2,
    layer.top + layer.height / 2
  )
  expect(element(`${view} .cal-hint-layer`).contains(label)).toBe(false)
}

/** What a click at the middle of `rect` lands on. */
function hitAt(rect: DOMRect): Element | null {
  return document.elementFromPoint(rect.left + rect.width / 2, rect.top + rect.height / 2)
}

describe('the calendar hint in a browser', APP_RENDER_TEST, () => {
  it('covers the visible days of the week below their headers, the hint in their middle, at any scroll', async () => {
    const scroller = await open(WEEK, 'app-week-view')
    expectInPlace('app-week-view', scroller)

    for (const top of [0, scroller.scrollHeight]) {
      scroller.scrollTop = top
      await frames()
      expectInPlace('app-week-view', scroller)
    }
  })

  it('leaves the toolbar and the day headers usable, and takes the pointer from the grid', async () => {
    const scroller = await open(WEEK, 'app-week-view')
    const layer = element('app-week-view .cal-hint-layer')
    const toolbar = element('app-week-view .cal-nav').getBoundingClientRect()
    const headers = element('app-week-view .week-col-headers')

    expect(layer.getBoundingClientRect().top).toBeGreaterThanOrEqual(toolbar.bottom)
    const previous = element('app-week-view .cal-nav button')
    expect(previous.contains(hitAt(previous.getBoundingClientRect()))).toBe(true)
    expect(headers.contains(hitAt(headers.getBoundingClientRect()))).toBe(true)

    // The columns' corner, away from the hint: the layer takes the click.
    const grid = visibleColumns('app-week-view', scroller)
    const corner = document.elementFromPoint(grid.left + 8, grid.bottom - 8)
    expect(corner).toBe(layer)
    const button = element('app-week-view app-calendar-hint button')
    expect(button.contains(hitAt(button.getBoundingClientRect()))).toBe(true)
  })

  it('covers the visible day’s column, without a header, at any scroll', async () => {
    const scroller = await open(DAY, 'app-day-view')
    expectInPlace('app-day-view', scroller)

    scroller.scrollTop = 0
    await frames()
    expectInPlace('app-day-view', scroller)
  })

  it('keeps the hint within the screen on a phone', async () => {
    const scroller = await open(WEEK, 'app-week-view', 390, 844)
    expectInPlace('app-week-view', scroller)

    const hint = element('app-week-view app-calendar-hint').getBoundingClientRect()
    expect(hint.left).toBeGreaterThanOrEqual(0)
    expect(hint.right).toBeLessThanOrEqual(innerWidth)
    const card = element('app-week-view app-calendar-hint')
    expect(card.scrollWidth).toBeLessThanOrEqual(card.clientWidth + 1)
  })
})
