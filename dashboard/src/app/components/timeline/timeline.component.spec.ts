import { signal, type Provider, type WritableSignal } from '@angular/core'
import type { ConnectionKey, EntryKey } from '../../providers/connection'
import type { DayGroup } from '../../models/day-group'
import type { EntryUiAdapter } from '../../providers/types'
import type { ActivityEntry } from '../../generated/activity'
import type { CollectionStatus } from '../../services/types'
import type { SettingsSelection } from '../settings-dialog/settings-dialog.types'
import type { ViewMode } from '../../utils/app-url'
import { LiveAnnouncer, type AriaLivePoliteness } from '@angular/cdk/a11y'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createEntryAdapter } from '../../providers/provider-adapter'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection, type RangeCoverage } from '../../services/activity-selection.service'
import {
  ActivityViewStateService,
  SEARCH_URL_DELAY_MS,
  type DayRange,
} from '../../services/activity-view-state.service'
import { activityLog, meetingWeightOf } from '../../testing/activity-fixtures'
import { addDaysToKey } from '../../utils/local-day'
import { stubLayoutObservers } from '../../testing/observers'
import { BackendFailure } from '../../services/backend/backend-failure'
import { failureOf } from '../../services/failure'
import { timelineEmptyView, type TimelineEmptyState } from './timeline-empty-state'
import { TimelineComponent } from './timeline.component'

/** What the timeline reads of its three services, faked: each member typed as the real one. */
interface TimelineActivityMock {
  isLoading: WritableSignal<boolean>
  groupedByDay: WritableSignal<readonly DayGroup<EntryUiAdapter>[]>
  allEntries: WritableSignal<ActivityEntry[]>
  searchQuery: WritableSignal<string>
  setSearchQuery: (query: string) => void
  currentView: WritableSignal<ViewMode>
  settings: WritableSignal<SettingsSelection | null>
  detailsOpen: WritableSignal<boolean>
  collectionStatus: WritableSignal<CollectionStatus>
  anySourceReady: WritableSignal<boolean>
  activeConnections: WritableSignal<Set<ConnectionKey>>
  openEvent: ReturnType<typeof vi.fn<(key: EntryKey) => Promise<boolean>>>
  rangeCoverage: WritableSignal<RangeCoverage>
  collectShownRange: ReturnType<typeof vi.fn<() => Promise<void> | null>>
  collectRange: ReturnType<typeof vi.fn<(range: DayRange) => Promise<void>>>
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
      allEntries: signal(activityLog.entries),
      groupedByDay: signal<readonly DayGroup<EntryUiAdapter>[]>([
        { date: '2026-05-18', entries: [entry] },
      ]),
      searchQuery,
      setSearchQuery: query => searchQuery.set(query),
      currentView: signal<ViewMode>('list'),
      settings: signal<SettingsSelection | null>(null),
      detailsOpen: signal(false),
      collectionStatus: signal<CollectionStatus>(overrides.status ?? { kind: 'idle' }),
      anySourceReady: signal(true),
      activeConnections: signal(new Set<ConnectionKey>(['git/default'])),
      openEvent: vi.fn((_key: EntryKey) => Promise.resolve(true)),
      rangeCoverage: signal<RangeCoverage>({ kind: 'covered' }),
      collectShownRange: vi.fn(() => Promise.resolve()),
      collectRange: vi.fn((_range: DayRange) => Promise.resolve()),
    },
  }
}

/** The services the timeline reads, each faked with only what it takes from it. */
function servicesOf(activity: TimelineActivityMock): Provider[] {
  const selection = {
    groupedByDay: activity.groupedByDay,
    rangeCoverage: activity.rangeCoverage,
  } satisfies Partial<ActivitySelection>
  const viewState = {
    searchQuery: activity.searchQuery,
    setSearchQuery: activity.setSearchQuery,
    currentView: activity.currentView,
    settings: activity.settings,
    detailsOpen: activity.detailsOpen,
    activeConnections: activity.activeConnections,
    openEvent: activity.openEvent,
  } satisfies Partial<ActivityViewStateService>
  const data = {
    allEntries: activity.allEntries,
    isLoading: activity.isLoading,
    collectionStatus: activity.collectionStatus,
    anySourceReady: activity.anySourceReady,
    collectShownRange: activity.collectShownRange,
    collectRange: activity.collectRange,
  } satisfies Partial<ActivityDataService>
  return [
    { provide: ActivitySelection, useValue: selection },
    { provide: ActivityViewStateService, useValue: viewState },
    { provide: ActivityDataService, useValue: data },
  ]
}

/** The title and message of the empty state on screen; null without one. */
function shownEmptyState(): { readonly title: string; readonly message: string | null } | null {
  const state = document.querySelector('app-empty-state')
  if (state === null) return null
  return {
    title: state.querySelector('.heading')?.textContent?.trim() ?? '',
    message: state.querySelector('.message')?.textContent?.trim() ?? null,
  }
}

/** The words `state` shows, to compare `shownEmptyState()` with: specs name the case, not its text. */
function emptyWords(state: TimelineEmptyState): {
  readonly title: string
  readonly message: string | null
} {
  const { title, message } = timelineEmptyView(state)
  return { title, message }
}

/** Two days with events: the Jira update on 18 May, the Git commit on 17 May. */
function twoDays(activity: TimelineActivityMock): void {
  const [git, jira] = activityLog.entries.map(createEntryAdapter)
  activity.groupedByDay.set([
    { date: '2026-05-18', entries: [jira] },
    { date: '2026-05-17', entries: [git] },
  ])
}

/** Gives the scroller a box so that the virtualizer draws rows in jsdom. */
function stubViewport(): void {
  vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(600)
  vi.spyOn(Element.prototype, 'clientWidth', 'get').mockReturnValue(800)
}

/** Makes even the two one-row days taller than the visible scroller. */
function stubOverflow(): void {
  vi.spyOn(Element.prototype, 'clientHeight', 'get').mockReturnValue(100)
}

/** `count` days with one Git commit each, newest first, from 18 May 2026 back. */
function manyDays(activity: TimelineActivityMock, count: number): void {
  const [git] = activityLog.entries
  activity.groupedByDay.set(
    Array.from({ length: count }, (_, index) => {
      const date = addDaysToKey('2026-05-18', -index)
      const entry = createEntryAdapter({ ...git, id: date, timestamp: `${date}T09:00:00Z` })
      return { date, entries: [entry] }
    })
  )
}

describe('TimelineComponent', () => {
  beforeEach(() => {
    stubLayoutObservers()
    stubViewport()
  })
  afterEach(() => {
    vi.restoreAllMocks()
    vi.unstubAllGlobals()
  })

  it('allows keyboard activation of entries', async () => {
    const { activity, entry } = mockActivity()

    await render(TimelineComponent, {
      providers: servicesOf(activity),
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
      providers: servicesOf(activity),
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
      providers: servicesOf(activity),
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
      providers: servicesOf(activity),
    })

    expect(shownEmptyState()).toEqual(emptyWords({ reason: 'failed', cause: 'unfinished' }))
  })

  it.each([
    ['a collect refused as invalid asks for another range', 'collect', 'refusedRange'],
    ['a state read refused as invalid offers no retry', 'refusedRead', 'refusedRead'],
  ] as const)('%s', async (_name, origin, cause) => {
    const refused = failureOf(new BackendFailure('invalidRequest', 'refused'))
    const { activity } = mockActivity({
      status: { kind: 'failed', operation: 'collect', origin, failure: refused },
    })
    activity.groupedByDay.set([])

    await render(TimelineComponent, {
      providers: servicesOf(activity),
    })

    expect(shownEmptyState()).toEqual(emptyWords({ reason: 'failed', cause }))
  })

  it('offers to collect this tab’s range where the collection on screen leaves it out', async () => {
    const { activity } = mockActivity()
    activity.groupedByDay.set([])
    const missing = {
      collected: { from: '2026-05-01', to: '2026-05-07' },
      own: { from: '2026-05-10', to: '2026-05-17' },
    } as const
    activity.rangeCoverage.set({ kind: 'other', ...missing, overlaps: false })

    await render(TimelineComponent, { providers: servicesOf(activity) })

    expect(shownEmptyState()).toEqual(emptyWords({ reason: 'notCollected', ...missing }))
    await userEvent.click(screen.getByRole('button', { name: 'Collect 10 May – 17 May 2026' }))
    expect(activity.collectRange).toHaveBeenCalledExactlyOnceWith(missing.own)
  })

  it('asks for a wider range where the collection covers this tab’s range', async () => {
    const { activity } = mockActivity()
    activity.groupedByDay.set([])

    await render(TimelineComponent, { providers: servicesOf(activity) })

    expect(shownEmptyState()).toEqual(emptyWords({ reason: 'nothingRecorded' }))
    expect(screen.queryByRole('button', { name: /^Collect/ })).toBeNull()
  })

  describe('the summary read out', () => {
    const DELAY = 200

    /** `LiveAnnouncer.announce` as the timeline calls it. */
    type Announce = (message: string, politeness: AriaLivePoliteness) => Promise<void>

    async function renderAnnounced(activity: TimelineActivityMock): Promise<{
      announce: ReturnType<typeof vi.fn<Announce>>
      settle: () => Promise<void>
    }> {
      const announce = vi.fn<Announce>(() => Promise.resolve())
      const { fixture } = await render(TimelineComponent, {
        providers: [
          ...servicesOf(activity),
          { provide: LiveAnnouncer, useValue: { announce } },
          { provide: SEARCH_URL_DELAY_MS, useValue: DELAY },
        ],
      })
      const settle = async (): Promise<void> => {
        fixture.detectChanges()
        await vi.advanceTimersByTimeAsync(DELAY)
        fixture.detectChanges()
      }
      await settle()
      announce.mockClear()
      return { announce, settle }
    }

    beforeEach(() => vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] }))
    afterEach(() => vi.useRealTimers())

    it('reads the summary out once the filter settles, not on every key', async () => {
      const { activity } = mockActivity()
      const { announce, settle } = await renderAnnounced(activity)
      const user = userEvent.setup({ advanceTimers: ms => vi.advanceTimersByTime(ms), delay: 50 })

      await user.type(screen.getByRole('searchbox'), 'abc')
      expect(announce).not.toHaveBeenCalled()
      await settle()

      expect(announce.mock.calls).toEqual([['1 event across 1 day matching “abc”', 'polite']])
      expect(document.querySelector('.timeline-summary')?.closest('[aria-live]')).toBeNull()
    })

    it('reads out only the result of a collection, not what it brings on the way', async () => {
      const { activity, entry } = mockActivity()
      const { announce, settle } = await renderAnnounced(activity)

      activity.isLoading.set(true)
      activity.groupedByDay.set([{ date: '2026-05-18', entries: [entry, entry] }])
      await settle()
      expect(announce).not.toHaveBeenCalled()

      activity.isLoading.set(false)
      activity.groupedByDay.set([
        { date: '2026-05-18', entries: [entry, entry] },
        { date: '2026-05-17', entries: [entry] },
      ])
      await settle()

      expect(announce.mock.calls).toEqual([['3 events across 2 days', 'polite']])
    })

    it.each<[string, (activity: TimelineActivityMock) => void]>([
      ['another view', (activity): void => activity.currentView.set('week')],
      ['the settings', (activity): void => activity.settings.set('general')],
      ['the details', (activity): void => activity.detailsOpen.set(true)],
    ])('reads nothing out with %s on screen', async (_name, hide) => {
      const { activity, entry } = mockActivity()
      const { announce, settle } = await renderAnnounced(activity)

      hide(activity)
      activity.groupedByDay.set([
        { date: '2026-05-18', entries: [entry] },
        { date: '2026-05-17', entries: [entry] },
      ])
      await settle()

      expect(announce).not.toHaveBeenCalled()
    })
  })

  it('labels the field as a filter', async () => {
    const { activity } = mockActivity()

    await render(TimelineComponent, {
      providers: servicesOf(activity),
    })

    const field = screen.getByRole('searchbox', { name: 'Filter activity' })
    expect(field.getAttribute('placeholder')).toBe('Filter activity…')
  })

  it('offers to clear a filter that matches nothing', async () => {
    const { activity } = mockActivity({ query: 'nope' })
    activity.groupedByDay.set([])

    await render(TimelineComponent, {
      providers: servicesOf(activity),
    })

    expect(shownEmptyState()).toEqual(emptyWords({ reason: 'noMatches', query: 'nope' }))

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
      providers: servicesOf(activity),
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
      providers: servicesOf(activity),
    })
    await fixture.whenStable()

    expect(screen.queryByRole('slider')).toBeNull()
  })

  it('offers a day index when two days overflow the list', async () => {
    stubOverflow()
    const { activity } = mockActivity()
    twoDays(activity)

    await render(TimelineComponent, {
      providers: servicesOf(activity),
    })

    const rail = await screen.findByRole('slider', { name: 'Jump to day' })
    expect(rail.getAttribute('aria-valuetext')).toBe('Monday, 18 May 2026, 1 event')
  })

  it('spans the day index over the days the list shows, not the whole range', async () => {
    stubOverflow()
    const { activity } = mockActivity()
    // The collected range reaches far past them; the rail is drawn from the days alone.
    twoDays(activity)

    const { fixture } = await render(TimelineComponent, {
      providers: servicesOf(activity),
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
      providers: servicesOf(activity),
    })
    const rail = await screen.findByRole('slider')
    rail.focus()
    await userEvent.keyboard('{Enter}')

    expect(document.activeElement).toBe(screen.getByRole('button', { name: /open jira event/i }))
  })

  describe('a day whose rows change', () => {
    async function renderDay(): Promise<{
      activity: TimelineActivityMock
      gainRow: () => Promise<void>
    }> {
      const { activity } = mockActivity()
      const [git, jira] = activityLog.entries.map(createEntryAdapter)
      activity.groupedByDay.set([{ date: '2026-05-18', entries: [git!] }])
      activity.allEntries.set([activityLog.entries[0]])
      const { fixture } = await render(TimelineComponent, { providers: servicesOf(activity) })
      return {
        activity,
        gainRow: async (): Promise<void> => {
          activity.groupedByDay.set([{ date: '2026-05-18', entries: [jira!, git!] }])
          activity.allEntries.set(activityLog.entries)
          await fixture.whenStable()
        },
      }
    }

    it('keeps its list and fades in an entry added by a collection', async () => {
      const { gainRow } = await renderDay()
      const list = document.querySelector('.day-group ol')

      await gainRow()

      expect(document.querySelector('.day-group ol')).toBe(list)
      expect(list?.querySelectorAll('.entry')).toHaveLength(2)
      expect(list?.querySelector('.entry-added .entry')?.getAttribute('aria-label')).toMatch(
        /open jira event/i
      )
    })

    it('keeps the focus on an entry as its day gains a row', async () => {
      const { gainRow } = await renderDay()
      const entry = screen.getByRole('button', { name: /open git event/i })
      entry.focus()

      await gainRow()

      expect(document.activeElement).toBe(entry)
    })
  })

  it('moves focus between the days with Page Down and Page Up, as a feed does', async () => {
    const { activity } = mockActivity()
    twoDays(activity)
    await render(TimelineComponent, { providers: servicesOf(activity) })
    const [first, second] = screen.getAllByRole('article')
    screen.getByRole('button', { name: /open jira event/i }).focus()

    await userEvent.keyboard('{PageDown}')
    expect(document.activeElement).toBe(second)

    await userEvent.keyboard('{PageUp}')
    expect(document.activeElement).toBe(first)
    // Nothing comes before the first day.
    await userEvent.keyboard('{PageUp}')
    expect(document.activeElement).toBe(first)
  })

  it('draws only the days near the screen, each saying where it is in the list', async () => {
    const { activity } = mockActivity()
    manyDays(activity, 60)

    await render(TimelineComponent, { providers: servicesOf(activity) })

    const feed = screen.getByRole('feed', { name: 'Activity by day' })
    const days = screen.getAllByRole('article')
    expect(days.length).toBeGreaterThan(0)
    expect(days.length).toBeLessThan(60)
    expect(days.map(day => day.getAttribute('aria-posinset'))).toEqual(
      days.map((_, index) => String(index + 1))
    )
    expect(days.every(day => day.getAttribute('aria-setsize') === '60')).toBe(true)
    expect(feed.contains(days[0]!)).toBe(true)
    // The summary counts every day, drawn or not.
    expect(screen.getByText('60 events across 60 days')).toBeTruthy()
  })

  it('moves "Today" to the next day at midnight', async () => {
    vi.useFakeTimers({ toFake: ['Date', 'setTimeout', 'clearTimeout'], shouldAdvanceTime: true })
    vi.setSystemTime(new Date(2026, 4, 18, 23, 59, 30))
    try {
      const { activity } = mockActivity()
      const { fixture } = await render(TimelineComponent, {
        providers: servicesOf(activity),
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
