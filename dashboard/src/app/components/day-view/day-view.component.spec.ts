import { render, screen, type RenderResult } from '@testing-library/angular'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { BusyStatus } from '../../generated/activity'
import { createEntryAdapter } from '../../providers/provider-adapter'
import type { EntryUiAdapter } from '../../providers/types'
import { meetingWeightOf } from '../../testing/activity-fixtures'
import { MIN_BLOCK_HEIGHT } from '../calendar/calendar-grid'
import {
  fakeCalendarState,
  hintWords,
  shownHint,
  type CalendarStateInit,
  type FakeCalendarState,
} from '../calendar/calendar-testing'
import { DayViewComponent } from './day-view.component'

function meeting(
  title: string,
  hour: number,
  minutes: number,
  busyStatus?: BusyStatus
): EntryUiAdapter {
  return createEntryAdapter({
    source: 'calendar',
    connection: 'work',
    id: title,
    timestamp: new Date(2026, 10, 17, hour).toISOString(),
    title,
    durationMinutes: minutes,
    busyStatus,
    meetingWeight: meetingWeightOf(busyStatus),
  })
}

async function renderDay(init: CalendarStateInit): Promise<{
  state: FakeCalendarState
  result: RenderResult<DayViewComponent>
}> {
  const state = fakeCalendarState({ view: 'day', ...init })
  return { state, result: await render(DayViewComponent, { providers: state.providers }) }
}

describe('DayViewComponent', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] })
    vi.setSystemTime(new Date(2026, 10, 17, 16, 59, 30))
  })
  afterEach(() => vi.useRealTimers())

  it('shows meeting time and the current-time line on today', async () => {
    const { container, fixture } = (
      await renderDay({
        entries: [meeting('Planning', 9, 90), meeting('Maybe', 10, 60, 'tentative')],
        day: '2026-11-17',
      })
    ).result

    expect(screen.getByLabelText('Meetings: 1h 30m · +30m tentative')).toBeTruthy()
    expect(
      screen.getByRole('button', { name: 'Open event: Maybe (tentative)' }).classList
    ).toContain('is-tentative')
    expect(container.querySelector('.now-label')!.textContent!.trim()).toBe('16:59')

    vi.advanceTimersByTime(30_000)
    await fixture.whenStable()

    expect(container.querySelector('.now-label')!.textContent!.trim()).toBe('17:00')
    expect(container.querySelector<HTMLElement>('.now-line')!.style.top).toBe(`${17 * 64}px`)
    // The time label sits on the hour label: only the time shows.
    const covered = [...container.querySelectorAll('.hour-label.is-covered')]
    expect(covered.map(label => label.textContent!.trim())).toEqual(['17:00'])
  })

  it('draws a meeting ending at midnight as a block, one past midnight as a bar', async () => {
    const { container } = (
      await renderDay({
        entries: [meeting('Until midnight', 23, 60), meeting('Past midnight', 23, 90)],
        day: '2026-11-17',
      })
    ).result

    const button = (name: string): HTMLElement =>
      screen.getByRole('button', { name: `Open event: ${name}` })
    expect(button('Until midnight').classList).toContain('cal-event')
    expect(button('Past midnight').classList).toContain('cal-allday-bar')
    expect(container.querySelectorAll('.cal-allday-bar')).toHaveLength(1)
  })

  it('has no line and no meeting time on another day without meetings', async () => {
    const { container } = (await renderDay({ day: '2026-11-16' })).result

    expect(container.querySelector('.now-line')).toBeNull()
    expect(container.querySelector('.cal-meeting-time')).toBeNull()
  })

  it('zooms in steps, shows the scale and stops at the ends', async () => {
    const {
      state,
      result: { fixture },
    } = await renderDay({ day: '2026-11-16' })
    const zoomIn = screen.getByRole('button', { name: 'Zoom in' })
    const zoomOut = screen.getByRole('button', { name: 'Zoom out' })
    const readout = (): HTMLButtonElement =>
      screen.getByRole('button', { name: /^Zoom \d+%, reset to 100%$/ })

    expect(readout().textContent?.trim()).toBe('100%')
    expect(readout().disabled).toBe(true)

    zoomIn.click()
    fixture.detectChanges()
    expect(state.hourHeight()).toBe(80)
    expect(readout().textContent?.trim()).toBe('125%')
    expect(readout().disabled).toBe(false)

    readout().click()
    fixture.detectChanges()
    expect(state.hourHeight()).toBe(64)

    for (let press = 0; press < 5; press++) zoomOut.click()
    fixture.detectChanges()
    expect(state.hourHeight()).toBe(32)
    expect(zoomOut.hasAttribute('disabled')).toBe(true)
  })

  it('ends a block on the row of its end time on a daylight saving day', async () => {
    // 28 March 2027 in Berlin: 01:30 CET to 03:30 CEST is one real hour, two on the clock.
    const night = createEntryAdapter({
      source: 'calendar',
      connection: 'work',
      id: 'night',
      timestamp: '2027-03-28T00:30:00Z',
      title: 'Night deploy',
      durationMinutes: 60,
      meetingWeight: 'busy',
    })
    await renderDay({ entries: [night], day: '2027-03-28', timeZone: 'Europe/Berlin' })

    const block = screen.getByRole('button', { name: 'Open event: Night deploy' })
    expect(block.style.top).toBe(`${1.5 * 64}px`)
    expect(block.style.height).toBe(`${2 * 64}px`)
  })

  it('draws short meetings at the calendar’s minimum height', async () => {
    await renderDay({ entries: [meeting('Check-in', 9, 5)], day: '2026-11-17' })

    const block = screen.getByRole('button', { name: 'Open event: Check-in' })
    expect(block.style.height).toBe(`${MIN_BLOCK_HEIGHT}px`)
  })

  it('opens the event of a block and the bar of a meeting past midnight', async () => {
    const block = meeting('Planning', 9, 60)
    const bar = meeting('Past midnight', 23, 90)
    const { state } = await renderDay({ entries: [block, bar], day: '2026-11-17' })

    screen.getByRole('button', { name: 'Open event: Planning' }).click()
    screen.getByRole('button', { name: 'Open event: Past midnight' }).click()

    expect(state.opened).toEqual([block.trackKey, bar.trackKey])
  })

  it('says an empty day was not collected when it lies outside the collection', async () => {
    await renderDay({ day: '2026-11-10', from: '2026-11-16', to: '2026-11-22' })
    expect(shownHint()).toEqual(
      hintWords({
        reason: 'notCollected',
        unit: 'day',
        onScreen: { from: '2026-11-16', to: '2026-11-22' },
        collect: null,
      })
    )
  })

  it('offers to collect this tab’s range where another collection on screen leaves the day out', async () => {
    const { state } = await renderDay({
      day: '2026-11-17',
      from: '2026-11-16',
      to: '2026-11-22',
      coverage: {
        kind: 'other',
        collected: { from: '2026-11-01', to: '2026-11-05' },
        own: { from: '2026-11-16', to: '2026-11-22' },
        overlaps: false,
      },
    })
    expect(shownHint()).toEqual(
      hintWords({
        reason: 'notCollected',
        unit: 'day',
        onScreen: { from: '2026-11-01', to: '2026-11-05' },
        collect: { from: '2026-11-16', to: '2026-11-22' },
      })
    )

    const collect = screen.getByRole('button', { name: 'Collect 16 Nov – 22 Nov 2026' })
    // Only the hint's text is announced; the button is not part of the live region.
    expect(screen.getByRole('status').contains(collect)).toBe(false)
    collect.click()
    expect(state.collects()).toBe(1)
  })

  it('says a collected day without entries had no activity', async () => {
    await renderDay({ day: '2026-11-17', from: '2026-11-16', to: '2026-11-22' })
    expect(shownHint()).toEqual(hintWords({ reason: 'noActivity', unit: 'day', whole: true }))
  })
})
