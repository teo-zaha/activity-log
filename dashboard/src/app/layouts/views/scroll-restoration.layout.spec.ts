/// <reference types="@vitest/browser-playwright" />
// Where the calendar views are scrolled to across navigations, in a real browser (`ng test
// --configuration layout`, Chromium): the router restores them only once they are laid out.
import { Location } from '@angular/common'
import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { page } from 'vitest/browser'
import { DEFAULT_SCROLL_HOUR } from '../../components/calendar/calendar-grid'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { openApp } from '../../testing/app'

const WEEK = '2026-05-18'

function grid(view: 'app-week-view' | 'app-day-view'): HTMLElement {
  const element = document.querySelector<HTMLElement>(`${view} .cal-scroll`)
  if (!element?.isConnected) throw new Error(`${view} is on screen`)
  return element
}

/** Where a calendar view starts: its default hour. */
function startOf(): number {
  return DEFAULT_SCROLL_HOUR * TestBed.inject(ActivityViewStateService).hourHeight()
}

/** Two frames: what a change lays out and what observes it has run. */
async function frames(): Promise<void> {
  for (let i = 0; i < 2; i++) await new Promise(resolve => requestAnimationFrame(resolve))
}

async function openWeek(): Promise<void> {
  await page.viewport(1280, 800)
  await openApp(`/week/${WEEK}`)
  await expect.poll(() => grid('app-week-view').scrollTop).toBe(startOf())
}

describe('the calendar views across navigations', () => {
  it('shows a week where it was scrolled to on Back from the day', async () => {
    await openWeek()
    const left = document.querySelector('app-week-view')
    grid('app-week-view').scrollTop = 150
    await frames()

    await TestBed.inject(ActivityViewStateService).openDay(WEEK)
    await expect.poll(() => document.querySelectorAll('app-week-view').length).toBe(0)
    await expect.poll(() => grid('app-day-view').scrollTop).toBe(startOf())
    TestBed.inject(Location).back()

    // A week view of its own, built again, and shown where the first one was left.
    await expect.poll(() => document.querySelectorAll('app-day-view').length).toBe(0)
    const shown = document.querySelector('app-week-view')
    expect(shown).not.toBeNull()
    expect(shown).not.toBe(left)
    await expect.poll(() => grid('app-week-view').scrollTop).toBe(150)
  })

  it('starts a week opened anew at the default hour', async () => {
    await openWeek()
    const views = TestBed.inject(ActivityViewStateService)
    grid('app-week-view').scrollTop = 150
    await frames()

    await views.openDay(WEEK)
    await frames()
    await views.setView('week')

    await expect.poll(() => document.querySelector('app-week-view .cal-scroll')).not.toBeNull()
    await expect.poll(() => grid('app-week-view').scrollTop).toBe(startOf())
  })

  it('keeps the hour shown as it steps to the next week, and Back keeps it too', async () => {
    await openWeek()
    const views = TestBed.inject(ActivityViewStateService)
    grid('app-week-view').scrollTop = 150
    await frames()

    views.navigateWeek(1)
    await expect.poll(() => views.selectedDay()).not.toBe(WEEK)
    for (let i = 0; i < 3; i++) await frames()
    // The same view, a week later, from the same hour (the all-day row may come or go above it).
    expect(Math.abs(grid('app-week-view').scrollTop - 150)).toBeLessThanOrEqual(40)

    grid('app-week-view').scrollTop = 300
    await frames()
    TestBed.inject(Location).back()
    await expect.poll(() => views.selectedDay()).toBe(WEEK)
    await expect.poll(() => grid('app-week-view').scrollTop).toBe(150)
  })
})
