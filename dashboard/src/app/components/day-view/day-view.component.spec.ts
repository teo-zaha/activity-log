import { computed, signal, type Signal, type WritableSignal } from '@angular/core'
import { render, screen } from '@testing-library/angular'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { EntryKey } from '../../providers/connection'
import type { BusyStatus } from '../../generated/activity'
import { createEntryAdapter } from '../../providers/provider-adapter'
import type { EntryUiAdapter } from '../../providers/types'
import { ActivityService } from '../../services/activity.service'
import { meetingWeightOf } from '../../testing/activity-fixtures'
import { MIN_BLOCK_HEIGHT } from '../calendar/calendar-grid'
import { CALENDAR_TIME_ZONE } from '../calendar/calendar-time-zone'
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

interface DayActivityMock {
  selectedDay: WritableSignal<string>
  calendarEntries: WritableSignal<EntryUiAdapter[]>
  dateFrom: WritableSignal<string>
  dateTo: WritableSignal<string>
  dayEntries: Signal<EntryUiAdapter[]>
  hourHeight: WritableSignal<number>
  openEvent: (key: EntryKey) => Promise<boolean>
  openGroup: (keys: readonly EntryKey[]) => Promise<boolean>
}

function mockActivity(entries: EntryUiAdapter[], day: string): DayActivityMock {
  const selectedDay = signal(day)
  const calendarEntries = signal(entries)
  return {
    selectedDay,
    calendarEntries,
    dateFrom: signal(''),
    dateTo: signal(''),
    dayEntries: computed(() => calendarEntries().filter(entry => entry.dayKey === selectedDay())),
    hourHeight: signal(64),
    openEvent: () => Promise.resolve(true),
    openGroup: () => Promise.resolve(true),
  }
}

describe('DayViewComponent', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout', 'Date'] })
    vi.setSystemTime(new Date(2026, 10, 17, 16, 59, 30))
  })
  afterEach(() => vi.useRealTimers())

  it('shows meeting time and the current-time line on today', async () => {
    const { container, fixture } = await render(DayViewComponent, {
      providers: [
        {
          provide: ActivityService,
          useValue: mockActivity(
            [meeting('Planning', 9, 90), meeting('Maybe', 10, 60, 'tentative')],
            '2026-11-17'
          ),
        },
      ],
    })

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
    const { container } = await render(DayViewComponent, {
      providers: [
        {
          provide: ActivityService,
          useValue: mockActivity(
            [meeting('Until midnight', 23, 60), meeting('Past midnight', 23, 90)],
            '2026-11-17'
          ),
        },
      ],
    })

    const button = (name: string): HTMLElement =>
      screen.getByRole('button', { name: `Open event: ${name}` })
    expect(button('Until midnight').classList).toContain('cal-event')
    expect(button('Past midnight').classList).toContain('cal-allday-bar')
    expect(container.querySelectorAll('.cal-allday-bar')).toHaveLength(1)
  })

  it('has no line and no meeting time on another day without meetings', async () => {
    const { container } = await render(DayViewComponent, {
      providers: [{ provide: ActivityService, useValue: mockActivity([], '2026-11-16') }],
    })

    expect(container.querySelector('.now-line')).toBeNull()
    expect(container.querySelector('.cal-meeting-time')).toBeNull()
  })

  it('zooms in steps, shows the scale and stops at the ends', async () => {
    const activity = mockActivity([], '2026-11-16')
    const { fixture } = await render(DayViewComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })
    const zoomIn = screen.getByRole('button', { name: 'Zoom in' })
    const zoomOut = screen.getByRole('button', { name: 'Zoom out' })
    const readout = (): HTMLButtonElement =>
      screen.getByRole('button', { name: /^Zoom \d+%, reset to 100%$/ })

    expect(readout().textContent?.trim()).toBe('100%')
    expect(readout().disabled).toBe(true)

    zoomIn.click()
    fixture.detectChanges()
    expect(activity.hourHeight()).toBe(80)
    expect(readout().textContent?.trim()).toBe('125%')
    expect(readout().disabled).toBe(false)

    readout().click()
    fixture.detectChanges()
    expect(activity.hourHeight()).toBe(64)

    for (let press = 0; press < 5; press++) zoomOut.click()
    fixture.detectChanges()
    expect(activity.hourHeight()).toBe(32)
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
    await render(DayViewComponent, {
      providers: [
        { provide: ActivityService, useValue: mockActivity([night], '2027-03-28') },
        { provide: CALENDAR_TIME_ZONE, useValue: 'Europe/Berlin' },
      ],
    })

    const block = screen.getByRole('button', { name: 'Open event: Night deploy' })
    expect(block.style.top).toBe(`${1.5 * 64}px`)
    expect(block.style.height).toBe(`${2 * 64}px`)
  })

  it('draws short meetings at the calendar’s minimum height', async () => {
    await render(DayViewComponent, {
      providers: [
        {
          provide: ActivityService,
          useValue: mockActivity([meeting('Check-in', 9, 5)], '2026-11-17'),
        },
      ],
    })

    const block = screen.getByRole('button', { name: 'Open event: Check-in' })
    expect(block.style.height).toBe(`${MIN_BLOCK_HEIGHT}px`)
  })
})
