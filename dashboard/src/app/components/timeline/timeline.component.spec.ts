import { signal, type WritableSignal } from '@angular/core'
import type { ConnectionKey, EntryKey } from '../../providers/connection'
import type { DayGroup } from '../../models/day-group'
import type { EntryUiAdapter } from '../../providers/types'
import type { CollectionStatus } from '../../services/types'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createEntryAdapter } from '../../providers/provider-adapter'
import { ActivityService } from '../../services/activity.service'
import { activityLog, meetingWeightOf } from '../../testing/activity-fixtures'
import { stubLayoutObservers } from '../../testing/observers'
import { BackendFailure } from '../../services/backend/backend-failure'
import { failureOf } from '../../services/failure'
import { TimelineComponent } from './timeline.component'

interface TimelineActivityMock {
  isLoading: WritableSignal<boolean>
  groupedByDay: WritableSignal<DayGroup<EntryUiAdapter>[]>
  searchQuery: WritableSignal<string>
  setSearchQuery: (query: string) => void
  dateFrom: WritableSignal<string>
  dateTo: WritableSignal<string>
  collectionStatus: WritableSignal<CollectionStatus>
  providers: WritableSignal<{ label: string }[]>
  activeConnections: WritableSignal<Set<ConnectionKey>>
  openEvent: ReturnType<typeof vi.fn<(key: EntryKey) => Promise<boolean>>>
}

function mockActivity(overrides: { status?: CollectionStatus; query?: string } = {}): {
  entry: EntryUiAdapter
  activity: TimelineActivityMock
} {
  const entry = createEntryAdapter(activityLog.entries[0])
  const searchQuery = signal(overrides.query ?? '')
  return {
    entry,
    activity: {
      isLoading: signal(false),
      groupedByDay: signal<DayGroup<EntryUiAdapter>[]>([{ date: '2026-05-18', entries: [entry] }]),
      searchQuery,
      setSearchQuery: query => searchQuery.set(query),
      dateFrom: signal('2026-05-17'),
      dateTo: signal('2026-05-18'),
      collectionStatus: signal<CollectionStatus>(overrides.status ?? { kind: 'idle' }),
      providers: signal([{ label: 'Git' }]),
      activeConnections: signal(new Set<ConnectionKey>(['git/default'])),
      openEvent: vi.fn((_key: EntryKey) => Promise.resolve(true)),
    },
  }
}

/** Two days with events: the Jira update on 18 May, the Git commit on 17 May. */
function twoDays(activity: TimelineActivityMock): void {
  const [git, jira] = activityLog.entries.map(createEntryAdapter)
  activity.groupedByDay.set([
    { date: '2026-05-18', entries: [jira] },
    { date: '2026-05-17', entries: [git] },
  ])
}

/** Makes every element report a list taller than its viewport. */
function stubOverflow(): void {
  vi.spyOn(Element.prototype, 'scrollHeight', 'get').mockReturnValue(2000)
  vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(600)
}

describe('TimelineComponent', () => {
  beforeEach(() => stubLayoutObservers())
  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('allows keyboard activation of entries', async () => {
    const { activity, entry } = mockActivity()

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    const button = screen.getByRole('button', { name: /open git event/i })
    button.focus()
    expect(button).toBe(document.activeElement)
    await userEvent.keyboard('{Enter}')

    expect(activity.openEvent).toHaveBeenCalledWith(entry.trackKey)
  })

  it('tags tentative and free meetings', async () => {
    const { activity } = mockActivity()
    const meeting = (title: string, busyStatus: 'tentative' | 'free' | 'busy'): EntryUiAdapter =>
      createEntryAdapter({
        source: 'calendar',
        connection: 'work',
        id: title,
        timestamp: '2026-05-18T09:00:00Z',
        title,
        durationMinutes: 30,
        busyStatus,
        meetingWeight: meetingWeightOf(busyStatus),
      })
    activity.groupedByDay.set([
      {
        date: '2026-05-18',
        entries: [meeting('Maybe', 'tentative'), meeting('Focus', 'free'), meeting('Sync', 'busy')],
      },
    ])

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    expect(screen.getByRole('button', { name: /Maybe \(tentative\)/ }).textContent).toContain(
      'Tentative'
    )
    expect(screen.getByRole('button', { name: /Focus \(free\)/ }).textContent).toContain('Free')
    const sync = screen.getByRole('button', { name: /event: Sync at/ })
    expect(sync.querySelector('.busy-tag')).toBeNull()
  })

  it('marks online meetings', async () => {
    const { activity } = mockActivity()
    const meeting = (title: string, joinUrl?: string): EntryUiAdapter =>
      createEntryAdapter({
        source: 'outlook',
        connection: 'default',
        id: title,
        timestamp: '2026-05-18T09:00:00Z',
        title,
        isOrganizer: false,
        durationMinutes: 30,
        busyStatus: 'tentative',
        meetingWeight: 'tentative',
        joinUrl,
      })
    activity.groupedByDay.set([
      {
        date: '2026-05-18',
        entries: [meeting('Call', 'https://zoom.us/j/1'), meeting('Room')],
      },
    ])

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    const call = screen.getByRole('button', { name: /Call \(tentative, online meeting\) at/ })
    expect(call.querySelector('.online-mark')).not.toBeNull()
    const room = screen.getByRole('button', { name: /Room \(tentative\) at/ })
    expect(room.querySelector('.online-mark')).toBeNull()
  })

  it('explains an empty result when the backend was lost', async () => {
    const { activity } = mockActivity({
      status: {
        kind: 'failed',
        operation: 'collect',
        origin: 'lostBackend',
        failure: failureOf(new BackendFailure('unreachable', 'offline')),
      },
    })
    activity.groupedByDay.set([])

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    expect(screen.getByText('No activity loaded')).toBeTruthy()
  })

  it.each([
    [
      'a collect refused as invalid asks for another range',
      'collect',
      'Choose another one',
      'Retry above',
    ],
    [
      'a state read refused as invalid offers no retry',
      'refusedRead',
      'check source status',
      'Retry above',
    ],
  ] as const)('%s', async (_name, origin, says, never) => {
    const refused = failureOf(new BackendFailure('invalidRequest', 'refused'))
    const { activity } = mockActivity({
      status: { kind: 'failed', operation: 'collect', origin, failure: refused },
    })
    activity.groupedByDay.set([])

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    expect(screen.getByText(new RegExp(says))).toBeTruthy()
    expect(screen.queryByText(new RegExp(never))).toBeNull()
  })

  it('labels the field as a filter', async () => {
    const { activity } = mockActivity()

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    const field = screen.getByRole('searchbox', { name: 'Filter activity' })
    expect(field.getAttribute('placeholder')).toBe('Filter activity…')
  })

  it('offers to clear a filter that matches nothing', async () => {
    const { activity } = mockActivity({ query: 'nope' })
    activity.groupedByDay.set([])

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    // The filter field's own clear button shares the label; use the empty-state action.
    const [, emptyStateAction] = screen.getAllByRole('button', { name: 'Clear filter' })
    await userEvent.click(emptyStateAction)
    expect(activity.searchQuery()).toBe('')
  })

  it('shows no day index while loading or empty', async () => {
    stubOverflow()
    const { activity } = mockActivity()
    activity.groupedByDay.set([])
    activity.isLoading.set(true)

    const { fixture } = await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })
    expect(screen.queryByRole('slider')).toBeNull()

    activity.isLoading.set(false)
    await fixture.whenStable()
    expect(screen.queryByRole('slider')).toBeNull()
  })

  it('shows no day index for a single day', async () => {
    stubOverflow()
    const { activity } = mockActivity()

    const { fixture } = await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })
    await fixture.whenStable()

    expect(screen.queryByRole('slider')).toBeNull()
  })

  it('offers a day index when two days overflow the list', async () => {
    stubOverflow()
    const { activity } = mockActivity()
    twoDays(activity)

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    const rail = await screen.findByRole('slider', { name: 'Jump to day' })
    expect(rail.getAttribute('aria-valuetext')).toBe('Monday, 18 May 2026, 1 event')
  })

  it('spans the day index over the days the list shows, not the whole range', async () => {
    stubOverflow()
    const { activity } = mockActivity()
    twoDays(activity)
    activity.dateFrom.set('2026-04-01')
    activity.dateTo.set('2026-05-31')

    const { fixture } = await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })
    const rail = await screen.findByRole('slider')
    await fixture.whenStable()

    expect(rail.querySelectorAll('.day')).toHaveLength(2)
  })

  it("moves focus to the active day's first event on Enter", async () => {
    stubOverflow()
    const { activity } = mockActivity()
    twoDays(activity)

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })
    const rail = await screen.findByRole('slider')
    rail.focus()
    await userEvent.keyboard('{Enter}')

    expect(document.activeElement).toBe(screen.getByRole('button', { name: /open jira event/i }))
  })

  it('corrects a jump while the days around it are drawn', async () => {
    stubOverflow()
    const { activity } = mockActivity()
    twoDays(activity)
    const { fixture } = await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })
    const rail = await screen.findByRole('slider')
    const host = fixture.nativeElement as HTMLElement
    const scroller = host.querySelector<HTMLElement>('.timeline-scroll')!
    const target = host.querySelector<HTMLElement>('[data-day="2026-05-17"]')!
    let scrollTop = 0
    Object.defineProperty(scroller, 'scrollTop', {
      get: () => scrollTop,
      set: (value: number) => (scrollTop = Math.max(0, value)),
    })
    // The target sits 1000 px down until the jump draws the day above it, 120 px taller than
    // its estimate.
    let grown = 0
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: Element
    ) {
      return new DOMRect(0, this === target ? 1000 + grown - scrollTop : 0, 100, 100)
    })
    const headerTop = parseFloat(getComputedStyle(host).getPropertyValue('--day-header-top')) || 0

    rail.focus()
    await userEvent.keyboard('{End}')
    expect(scrollTop).toBe(1000 - headerTop)

    grown = 120
    await vi.waitFor(() => expect(scrollTop).toBe(1120 - headerTop))
  })

  it('reads the active day from where the days are, not from the last one that crossed', async () => {
    stubOverflow()
    // The observer of the line under the day header (the one with a negative margin).
    type Crossing = (records: { target: Element | null; isIntersecting: boolean }[]) => void
    let crossed: Crossing | undefined
    vi.stubGlobal(
      'IntersectionObserver',
      class {
        constructor(callback: Crossing, options?: IntersectionObserverInit) {
          if (options?.rootMargin?.startsWith('-')) crossed = callback
        }
        observe(): void {}
        disconnect(): void {}
      }
    )
    const { activity } = mockActivity()
    twoDays(activity)
    const { fixture } = await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })
    const rail = await screen.findByRole('slider')
    const host = fixture.nativeElement as HTMLElement
    const first = host.querySelector('[data-day="2026-05-18"]')
    const second = host.querySelector('[data-day="2026-05-17"]')
    // Each day is 500 px tall below 40 px of padding; the scroller's top is at 0.
    let scrollTop = 0
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: Element
    ) {
      const index = this === first ? 0 : this === second ? 1 : -1
      return index < 0
        ? new DOMRect(0, 0, 100, 0)
        : new DOMRect(0, 40 + index * 500 - scrollTop, 100, 500)
    })
    await vi.waitFor(() => expect(crossed).toBeDefined())

    scrollTop = 800
    crossed?.([{ target: second, isIntersecting: true }])
    await vi.waitFor(() =>
      expect(rail.getAttribute('aria-valuetext')).toBe('Sunday, 17 May 2026, 1 event')
    )

    // Flung back to the top: the first day never crossed the line, which is in the padding now.
    scrollTop = 0
    crossed?.([{ target: second, isIntersecting: false }])
    await vi.waitFor(() =>
      expect(rail.getAttribute('aria-valuetext')).toBe('Monday, 18 May 2026, 1 event')
    )
  })

  it('makes the last day active at the end of the list, though it never reaches the line', async () => {
    stubOverflow()
    const { activity } = mockActivity()
    twoDays(activity)
    const { fixture } = await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })
    const rail = await screen.findByRole('slider')
    const host = fixture.nativeElement as HTMLElement
    const scroller = host.querySelector<HTMLElement>('.timeline-scroll')!
    const first = host.querySelector('[data-day="2026-05-18"]')
    const second = host.querySelector('[data-day="2026-05-17"]')
    // The first day is 1700 px tall, the last 200 px: at the end (1400) its top is at 340, far
    // below the line under the header.
    let scrollTop = 0
    vi.spyOn(scroller, 'scrollTop', 'get').mockImplementation(() => scrollTop)
    vi.spyOn(Element.prototype, 'getBoundingClientRect').mockImplementation(function (
      this: Element
    ) {
      if (this === first) return new DOMRect(0, 40 - scrollTop, 100, 1700)
      if (this === second) return new DOMRect(0, 1740 - scrollTop, 100, 200)
      return new DOMRect(0, 0, 100, 0)
    })

    scrollTop = 1000
    scroller.dispatchEvent(new Event('scroll'))
    await fixture.whenStable()
    expect(rail.getAttribute('aria-valuetext')).toBe('Monday, 18 May 2026, 1 event')

    scrollTop = 1400
    scroller.dispatchEvent(new Event('scroll'))
    await vi.waitFor(() =>
      expect(rail.getAttribute('aria-valuetext')).toBe('Sunday, 17 May 2026, 1 event')
    )
  })

  it('moves "Today" to the next day at midnight', async () => {
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'], shouldAdvanceTime: true })
    vi.setSystemTime(new Date(2026, 4, 18, 23, 59, 30))
    try {
      const { activity } = mockActivity()
      const { fixture } = await render(TimelineComponent, {
        providers: [{ provide: ActivityService, useValue: activity }],
      })
      expect(screen.getByText('Today')).toBeTruthy()

      await vi.advanceTimersByTimeAsync(60_000)
      await fixture.whenStable()

      expect(screen.queryByText('Today')).toBeNull()
      expect(screen.getByText('Yesterday')).toBeTruthy()
    } finally {
      vi.useRealTimers()
    }
  })
})
