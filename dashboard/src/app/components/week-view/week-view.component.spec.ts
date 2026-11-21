import { render, screen, type RenderResult } from '@testing-library/angular'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { BusyStatus } from '../../generated/activity'
import { createEntryAdapter } from '../../providers/provider-adapter'
import type { EntryUiAdapter } from '../../providers/types'
import { meetingWeightOf } from '../../testing/activity-fixtures'
import { MIN_BLOCK_HEIGHT } from '../calendar/calendar-grid'
import {
  fakeCalendarState,
  type CalendarStateInit,
  type FakeCalendarState,
} from '../calendar/calendar-testing'
import { WeekViewComponent } from './week-view.component'

// Tuesday 17 November 2026, 09:41 local time; the week runs Mon 16 to Sun 22.
const NOW = new Date(2026, 10, 17, 9, 41)

function meeting(
  title: string,
  day: number,
  hour: number,
  minutes: number,
  busyStatus?: BusyStatus
): EntryUiAdapter {
  return createEntryAdapter({
    source: 'outlook',
    connection: 'default',
    id: title,
    timestamp: new Date(2026, 10, day, hour).toISOString(),
    title,
    isOrganizer: false,
    durationMinutes: minutes,
    busyStatus,
    meetingWeight: meetingWeightOf(busyStatus),
  })
}

async function renderWeek(init: Partial<CalendarStateInit>): Promise<{
  state: FakeCalendarState
  result: RenderResult<WeekViewComponent>
}> {
  const state = fakeCalendarState({ day: '2026-11-17', ...init })
  return { state, result: await render(WeekViewComponent, { providers: state.providers }) }
}

describe('WeekViewComponent', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] })
    vi.setSystemTime(NOW)
  })
  afterEach(() => vi.useRealTimers())

  it('draws the current-time line in today’s column and moves it every minute', async () => {
    const { container, fixture } = (await renderWeek({})).result

    const lines = container.querySelectorAll<HTMLElement>('.now-line')
    expect(lines).toHaveLength(1)
    expect(lines[0].closest('.day-col')!.classList.contains('is-today')).toBe(true)
    expect(lines[0].style.top).toBe(`${(9 + 41 / 60) * 64}px`)
    expect(container.querySelector('.now-label')!.textContent!.trim()).toBe('09:41')
    // 41 minutes past is clear of both hour labels.
    expect(container.querySelector('.hour-label.is-covered')).toBeNull()

    vi.advanceTimersByTime(60_000)
    await fixture.whenStable()

    expect(container.querySelector('.now-label')!.textContent!.trim()).toBe('09:42')
  })

  it('hides the hour label under the time label on the full hour', async () => {
    vi.setSystemTime(new Date(NOW.getFullYear(), NOW.getMonth(), NOW.getDate(), 10, 0))
    const { container } = (await renderWeek({})).result

    const covered = [...container.querySelectorAll('.hour-label.is-covered')]
    expect(covered.map(label => label.textContent!.trim())).toEqual(['10:00'])
  })

  it('has no current-time line in another week', async () => {
    const { container } = (await renderWeek({ day: '2026-11-09' })).result

    expect(container.querySelector('.now-line')).toBeNull()
    expect(container.querySelector('.now-label')).toBeNull()
  })

  it('marks tentative and free meetings', async () => {
    await renderWeek({
      entries: [
        meeting('Maybe', 16, 9, 30, 'tentative'),
        meeting('Focus', 16, 11, 30, 'free'),
        meeting('Sync', 16, 13, 30, 'busy'),
      ],
    })

    const event = (name: string): HTMLElement => screen.getByRole('button', { name })
    expect(event('Open event: Maybe (tentative)').classList).toContain('is-tentative')
    expect(event('Open event: Focus (free)').classList).toContain('is-free')
    expect(event('Open event: Sync').classList).not.toContain('is-tentative')
    expect(event('Open event: Sync').classList).not.toContain('is-free')
  })

  it('shows meeting time in each day header and once for the week', async () => {
    const { container } = (
      await renderWeek({
        entries: [
          meeting('Planning', 16, 9, 120),
          meeting('Overlap', 16, 10, 60),
          meeting('Maybe', 16, 14, 60, 'tentative'),
          meeting('Free', 17, 9, 60, 'free'),
          meeting('Review', 18, 15, 90),
        ],
      })
    ).result

    expect(
      screen.getByRole('button', { name: /Mon 16, meetings 2h · \+1h tentative$/ })
    ).toBeTruthy()
    expect(screen.getByRole('button', { name: /Wed 18, meetings 1h 30m$/ })).toBeTruthy()
    // Only free time on Tuesday: nothing to show.
    expect(screen.getByRole('button', { name: /Tue 17$/ })).toBeTruthy()
    expect(container.querySelectorAll('.day-meeting-time')).toHaveLength(2)

    const week = screen.getByLabelText('Meetings this week: 3h 30m · +1h tentative')
    expect(week.textContent).toContain('3h 30m')
  })

  it('counts only the meetings the view shows', async () => {
    const {
      state,
      result: { fixture },
    } = await renderWeek({
      entries: [meeting('Planning', 16, 9, 60), meeting('Review', 18, 9, 60)],
    })
    expect(screen.getByLabelText('Meetings this week: 2h')).toBeTruthy()

    state.calendarEntries.set([state.calendarEntries()[0]])
    await fixture.whenStable()

    expect(screen.getByLabelText('Meetings this week: 1h')).toBeTruthy()
  })

  it('packs meetings past midnight into as few rows as they need, whatever their order', async () => {
    // Mon–Tue, Tue–Wed and Wed–Thu, listed newest first as the log has them: two rows do.
    const { container } = (
      await renderWeek({
        entries: [
          meeting('Wed night', 18, 20, 25 * 60),
          meeting('Tue night', 17, 20, 25 * 60),
          meeting('Mon night', 16, 20, 25 * 60),
        ],
      })
    ).result

    const bars = [...container.querySelectorAll<HTMLElement>('.cal-allday-bar')]
    expect(bars).toHaveLength(3)
    const rowOf = (title: string): string =>
      bars.find(bar => bar.textContent?.includes(title))?.style.top ?? ''
    expect(new Set(bars.map(bar => bar.style.top)).size).toBe(2)
    expect(rowOf('Mon night')).toBe(rowOf('Wed night'))
    expect(rowOf('Tue night')).not.toBe(rowOf('Mon night'))
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
    await renderWeek({ entries: [night], day: '2027-03-28', timeZone: 'Europe/Berlin' })

    const block = screen.getByRole('button', { name: 'Open event: Night deploy' })
    expect(block.style.top).toBe(`${1.5 * 64}px`)
    expect(block.style.height).toBe(`${2 * 64}px`)
  })

  it('draws short meetings at the calendar’s minimum height', async () => {
    await renderWeek({ entries: [meeting('Check-in', 16, 9, 5)] })

    const block = screen.getByRole('button', { name: 'Open event: Check-in' })
    expect(block.style.height).toBe(`${MIN_BLOCK_HEIGHT}px`)
  })

  it('lays out the work week five days wide with up to four blocks side by side', async () => {
    // Five at once on Monday.
    const crowd = ['A', 'B', 'C', 'D', 'E'].map(name => meeting(name, 16, 9, 60))
    const {
      state,
      result: { container, fixture },
    } = await renderWeek({ entries: crowd, view: 'workweek' })

    expect(container.querySelectorAll('.day-col')).toHaveLength(5)
    expect(container.querySelectorAll('.cal-event')).toHaveLength(3)
    screen.getByRole('button', { name: 'Open 2 additional events' }).click()
    expect(state.opened).toEqual([[crowd[3].trackKey, crowd[4].trackKey]])

    state.currentView.set('week')
    await fixture.whenStable()

    expect(container.querySelectorAll('.day-col')).toHaveLength(7)
    expect(screen.getByRole('button', { name: 'Open 3 additional events' })).toBeTruthy()
  })

  it('opens a day from its header', async () => {
    const { state } = await renderWeek({})

    screen.getByRole('button', { name: /^Open day view for Thu 19/ }).click()

    expect(state.selectedDay()).toBe('2026-11-19')
  })

  it('says an empty week outside the collection was not collected', async () => {
    await renderWeek({ day: '2026-11-10', from: '2026-11-16', to: '2026-11-22' })
    expect(screen.getByRole('status').querySelector('.hint-title')?.textContent).toBe(
      'This week was not collected'
    )
  })

  it('says a collected week without entries had no activity', async () => {
    // Only Sunday is collected: enough to say nothing happened in it.
    await renderWeek({ from: '2026-11-22', to: '2026-11-29' })
    expect(screen.getByRole('status').querySelector('.hint-title')?.textContent).toBe(
      'No activity in this week'
    )
  })

  it('totals meeting time only for the days another collection on screen holds', async () => {
    // This tab's range is the week; the collection on screen holds Monday and Tuesday.
    await renderWeek({
      entries: [meeting('Planning', 16, 9, 60), meeting('Late', 17, 23, 3 * 60)],
      from: '2026-11-16',
      to: '2026-11-22',
      coverage: {
        kind: 'other',
        collected: { from: '2026-11-16', to: '2026-11-17' },
        own: { from: '2026-11-16', to: '2026-11-22' },
        overlaps: true,
      },
    })

    // Wednesday's two hours of the late meeting are drawn, but not totalled.
    expect(screen.getByLabelText('Meetings this week: 2h')).toBeTruthy()
    expect(screen.getByRole('button', { name: /^Open day view for Wed 18$/ })).toBeTruthy()
  })

  it('offers to collect this tab’s range where another collection on screen leaves it out', async () => {
    const { state } = await renderWeek({
      from: '2026-11-16',
      to: '2026-11-22',
      coverage: {
        kind: 'other',
        collected: { from: '2026-11-02', to: '2026-11-08' },
        own: { from: '2026-11-16', to: '2026-11-22' },
        overlaps: false,
      },
    })

    const status = screen.getByRole('status')
    expect(status.querySelector('.hint-title')?.textContent).toBe('This week was not collected')
    expect(status.querySelector('.hint-message')?.textContent).toBe(
      'The activity on screen covers 2 Nov – 8 Nov 2026.'
    )
    screen.getByRole('button', { name: 'Collect 16 Nov – 22 Nov 2026' }).click()
    expect(state.collects()).toBe(1)
  })

  it('offers no collect where this tab’s range holds none of the week', async () => {
    await renderWeek({ day: '2026-11-10', from: '2026-11-16', to: '2026-11-22' })

    expect(screen.queryByRole('button', { name: /^Collect/ })).toBeNull()
  })
})
