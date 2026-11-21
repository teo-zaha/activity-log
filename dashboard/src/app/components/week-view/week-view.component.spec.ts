import { computed, signal, type Signal, type WritableSignal } from '@angular/core'
import { render, screen, type RenderResult } from '@testing-library/angular'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { EntryKey } from '../../providers/connection'
import type { BusyStatus } from '../../generated/activity'
import { createEntryAdapter } from '../../providers/provider-adapter'
import type { EntryUiAdapter } from '../../providers/types'
import { ActivityService } from '../../services/activity.service'
import { meetingWeightOf } from '../../testing/activity-fixtures'
import type { ViewMode } from '../../utils/app-url'
import { addDaysToKey, type DayKey } from '../../utils/local-day'
import { MIN_BLOCK_HEIGHT } from '../calendar/calendar-grid'
import { CALENDAR_TIME_ZONE } from '../calendar/calendar-time-zone'
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

interface WeekActivityMock {
  currentView: WritableSignal<ViewMode>
  selectedDay: WritableSignal<string>
  weekDays: Signal<string[]>
  workWeekDays: Signal<string[]>
  calendarEntries: WritableSignal<EntryUiAdapter[]>
  dateFrom: WritableSignal<string>
  dateTo: WritableSignal<string>
  weekEntriesByDay: Signal<Map<string, EntryUiAdapter[]>>
  hourHeight: WritableSignal<number>
  openEvent: (key: EntryKey) => Promise<boolean>
  openGroup: (keys: readonly EntryKey[]) => Promise<boolean>
}

function mockActivity(
  entries: EntryUiAdapter[],
  selectedDay: DayKey = '2026-11-17'
): WeekActivityMock {
  const selected = signal(selectedDay)
  const weekDays = computed(() => {
    const date = new Date(`${selected()}T12:00:00`)
    const monday = addDaysToKey(selected(), -((date.getDay() + 6) % 7))
    return Array.from({ length: 7 }, (_, i) => addDaysToKey(monday, i))
  })
  const calendarEntries = signal(entries)
  return {
    currentView: signal<ViewMode>('week'),
    selectedDay: selected,
    weekDays,
    workWeekDays: computed(() => weekDays().slice(0, 5)),
    calendarEntries,
    dateFrom: signal(''),
    dateTo: signal(''),
    weekEntriesByDay: computed(() => {
      const map = new Map<string, EntryUiAdapter[]>(weekDays().map(day => [day, []]))
      for (const entry of calendarEntries()) map.get(entry.dayKey)?.push(entry)
      return map
    }),
    hourHeight: signal(64),
    openEvent: () => Promise.resolve(true),
    openGroup: () => Promise.resolve(true),
  }
}

async function renderWeek(
  activity: WeekActivityMock,
  timeZone?: string
): Promise<RenderResult<WeekViewComponent>> {
  return render(WeekViewComponent, {
    providers: [
      { provide: ActivityService, useValue: activity },
      { provide: CALENDAR_TIME_ZONE, useValue: timeZone },
    ],
  })
}

describe('WeekViewComponent', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] })
    vi.setSystemTime(NOW)
  })
  afterEach(() => vi.useRealTimers())

  it('draws the current-time line in today’s column and moves it every minute', async () => {
    const { container, fixture } = await renderWeek(mockActivity([]))

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
    const { container } = await renderWeek(mockActivity([]))

    const covered = [...container.querySelectorAll('.hour-label.is-covered')]
    expect(covered.map(label => label.textContent!.trim())).toEqual(['10:00'])
  })

  it('has no current-time line in another week', async () => {
    const { container } = await renderWeek(mockActivity([], '2026-11-09'))

    expect(container.querySelector('.now-line')).toBeNull()
    expect(container.querySelector('.now-label')).toBeNull()
  })

  it('marks tentative and free meetings', async () => {
    await renderWeek(
      mockActivity([
        meeting('Maybe', 16, 9, 30, 'tentative'),
        meeting('Focus', 16, 11, 30, 'free'),
        meeting('Sync', 16, 13, 30, 'busy'),
      ])
    )

    const event = (name: string): HTMLElement => screen.getByRole('button', { name })
    expect(event('Open event: Maybe (tentative)').classList).toContain('is-tentative')
    expect(event('Open event: Focus (free)').classList).toContain('is-free')
    expect(event('Open event: Sync').classList).not.toContain('is-tentative')
    expect(event('Open event: Sync').classList).not.toContain('is-free')
  })

  it('shows meeting time in each day header and once for the week', async () => {
    const { container } = await renderWeek(
      mockActivity([
        meeting('Planning', 16, 9, 120),
        meeting('Overlap', 16, 10, 60),
        meeting('Maybe', 16, 14, 60, 'tentative'),
        meeting('Free', 17, 9, 60, 'free'),
        meeting('Review', 18, 15, 90),
      ])
    )

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
    const activity = mockActivity([meeting('Planning', 16, 9, 60), meeting('Review', 18, 9, 60)])
    const { fixture } = await renderWeek(activity)
    expect(screen.getByLabelText('Meetings this week: 2h')).toBeTruthy()

    activity.calendarEntries.set([activity.calendarEntries()[0]])
    await fixture.whenStable()

    expect(screen.getByLabelText('Meetings this week: 1h')).toBeTruthy()
  })

  it('packs meetings past midnight into as few rows as they need, whatever their order', async () => {
    // Mon–Tue, Tue–Wed and Wed–Thu, listed newest first as the log has them: two rows do.
    const { container } = await renderWeek(
      mockActivity([
        meeting('Wed night', 18, 20, 25 * 60),
        meeting('Tue night', 17, 20, 25 * 60),
        meeting('Mon night', 16, 20, 25 * 60),
      ])
    )

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
    await renderWeek(mockActivity([night], '2027-03-28'), 'Europe/Berlin')

    const block = screen.getByRole('button', { name: 'Open event: Night deploy' })
    expect(block.style.top).toBe(`${1.5 * 64}px`)
    expect(block.style.height).toBe(`${2 * 64}px`)
  })

  it('draws short meetings at the calendar’s minimum height', async () => {
    await renderWeek(mockActivity([meeting('Check-in', 16, 9, 5)]))

    const block = screen.getByRole('button', { name: 'Open event: Check-in' })
    expect(block.style.height).toBe(`${MIN_BLOCK_HEIGHT}px`)
  })
})
