import type { Type } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { screen } from '@testing-library/angular'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ActivityLog, OutlookEntry, ProvidersResponse } from '../generated/activity'
import { provideAppRouter } from '../app.routes'
import { ActivityDataService } from '../services/activity-data.service'
import { ActivitySelection } from '../services/activity-selection.service'
import { ActivityViewStateService } from '../services/activity-view-state.service'
import { MockBackendTransport, provideMockBackend } from '../testing/activity-fixtures'
import { calendarHintText } from './calendar/calendar-empty-hint'
import { hintWords, shownHint } from './calendar/calendar-testing'
import { DayViewComponent } from './day-view/day-view.component'
import { WeekViewComponent } from './week-view/week-view.component'

// The real services over the in-memory backend: what the views draw after a real load.

const checkedAt = '2026-11-17T08:00:00Z'
const providers: ProvidersResponse = {
  currentOs: 'windows',
  providers: [
    {
      connection: { source: 'outlook', name: 'default' },
      label: 'Outlook',
      status: 'available',
      usable: true,
      recheckable: false,
      supportedOs: ['macos', 'windows'],
      checkedAt,
    },
    {
      connection: { source: 'calendar', name: 'team' },
      label: 'Team calendar',
      status: 'available',
      usable: true,
      recheckable: false,
      supportedOs: ['linux', 'macos', 'windows'],
      checkedAt,
    },
  ],
}

function outlook(title: string, start: Date, durationMinutes: number): OutlookEntry {
  return {
    source: 'outlook',
    connection: 'default',
    id: title,
    timestamp: start.toISOString(),
    title,
    isOrganizer: false,
    durationMinutes,
    busyStatus: 'busy',
    meetingWeight: 'busy',
  }
}

/** Collected for Tue 17 November only (local days); the backend returns overlapping meetings. */
const log: ActivityLog = {
  meta: {
    generated: checkedAt,
    start: new Date(2026, 10, 17).toISOString(),
    end: new Date(2026, 10, 18).toISOString(),
  },
  summary: {
    connections: [
      { connection: { source: 'outlook', name: 'default' }, count: 2 },
      { connection: { source: 'calendar', name: 'team' }, count: 1 },
    ],
    mergedCount: 0,
  },
  entries: [
    // Mon 16, 23:00 to Tue 17, 01:00: an hour of it on the collected day.
    outlook('Late handover', new Date(2026, 10, 16, 23), 120),
    outlook('Planning', new Date(2026, 10, 17, 10), 30),
    {
      source: 'calendar',
      connection: 'team',
      id: 'team-sync',
      timestamp: new Date(2026, 10, 17, 14).toISOString(),
      title: 'Team sync',
      durationMinutes: 60,
      meetingWeight: 'busy',
    },
  ],
}

interface Loaded {
  readonly viewState: ActivityViewStateService
  readonly selection: ActivitySelection
}

async function load(): Promise<Loaded> {
  const backend = new MockBackendTransport()
  backend.providersResponse = providers
  backend.activityResponses = [log]
  TestBed.configureTestingModule({ providers: [provideMockBackend(backend), provideAppRouter()] })
  const viewState = TestBed.inject(ActivityViewStateService)
  await TestBed.inject(ActivityDataService).initLoad()
  await viewState.openDay('2026-11-17')
  return { viewState, selection: TestBed.inject(ActivitySelection) }
}

async function show(component: Type<unknown>): Promise<void> {
  const fixture = TestBed.createComponent(component)
  await fixture.whenStable()
}

describe('calendar views over a loaded log', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date(2026, 10, 17, 12))
  })
  afterEach(() => vi.useRealTimers())

  it('counts and draws a meeting that starts before the collected range', async () => {
    const { viewState } = await load()
    expect(viewState.dateFrom()).toBe('2026-11-17')

    await show(DayViewComponent)

    expect(screen.getByLabelText('Meetings: 2h 30m')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Open event: Late handover' })).toBeTruthy()
  })

  it('leaves out what the source filter hides', async () => {
    const { selection } = await load()
    selection.toggleConnection('calendar/team')

    await show(DayViewComponent)

    expect(screen.getByLabelText('Meetings: 1h 30m')).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Open event: Team sync' })).toBeNull()
  })

  it('shows meeting time in the week only for collected days', async () => {
    await load()

    await show(WeekViewComponent)

    expect(screen.getByRole('button', { name: /Tue 17, meetings 2h 30m$/ })).toBeTruthy()
    // Monday was not collected: its part of the handover is drawn but not totalled.
    expect(screen.getByRole('button', { name: /Mon 16$/ })).toBeTruthy()
    expect(screen.getByLabelText('Meetings this week: 2h 30m')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Open event: Late handover' })).toBeTruthy()
  })

  it('says a week outside the collected range was not collected, and offers to collect back to it', async () => {
    const { viewState } = await load()
    await viewState.openDay('2026-11-09')

    await show(WeekViewComponent)

    const hint = {
      reason: 'notCollected',
      unit: 'week',
      onScreen: { from: '2026-11-17', to: '2026-11-17' },
      collect: { from: '2026-11-09', to: '2026-11-17' },
    } as const
    expect(shownHint()).toEqual(hintWords(hint))
    expect(screen.getByRole('button', { name: calendarHintText(hint).action?.label })).toBeTruthy()
  })
})
