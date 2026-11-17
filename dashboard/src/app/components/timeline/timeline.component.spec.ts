import { signal, type WritableSignal } from '@angular/core'
import type { ConnectionKey } from '../../providers/connection'
import type { DayGroup } from '../../models/day-group'
import type { EntryUiAdapter } from '../../providers/types'
import type { CollectionStatus } from '../../services/types'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { createEntryAdapter } from '../../providers/provider-adapter'
import { ActivityService } from '../../services/activity.service'
import { activityLog } from '../../testing/activity-fixtures'
import { stubLayoutObservers } from '../../testing/observers'
import { TimelineComponent } from './timeline.component'

interface TimelineActivityMock {
  isLoading: WritableSignal<boolean>
  groupedByDay: WritableSignal<DayGroup<EntryUiAdapter>[]>
  searchQuery: WritableSignal<string>
  dateFrom: WritableSignal<string>
  dateTo: WritableSignal<string>
  collectionStatus: WritableSignal<CollectionStatus>
  providers: WritableSignal<{ label: string }[]>
  activeConnections: WritableSignal<Set<ConnectionKey>>
  selectedEntries: WritableSignal<EntryUiAdapter[] | null>
  selectedEntry: WritableSignal<EntryUiAdapter | null>
}

function mockActivity(overrides: { status?: CollectionStatus; query?: string } = {}): {
  entry: EntryUiAdapter
  activity: TimelineActivityMock
} {
  const entry = createEntryAdapter(activityLog.entries[0])
  return {
    entry,
    activity: {
      isLoading: signal(false),
      groupedByDay: signal<DayGroup<EntryUiAdapter>[]>([{ date: '2026-05-18', entries: [entry] }]),
      searchQuery: signal(overrides.query ?? ''),
      dateFrom: signal('2026-05-17'),
      dateTo: signal('2026-05-18'),
      collectionStatus: signal<CollectionStatus>(overrides.status ?? { kind: 'idle' }),
      providers: signal([{ label: 'Git' }]),
      activeConnections: signal(new Set<ConnectionKey>(['git/default'])),
      selectedEntries: signal<EntryUiAdapter[] | null>(null),
      selectedEntry: signal<EntryUiAdapter | null>(null),
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
    const selectedEntry = activity.selectedEntry

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    const button = screen.getByRole('button', { name: /open git event/i })
    button.focus()
    expect(button).toBe(document.activeElement)
    await userEvent.keyboard('{Enter}')

    expect(selectedEntry()).toBe(entry)
  })

  it('explains an empty result when collection timed out', async () => {
    const { activity } = mockActivity({
      status: {
        kind: 'timedOut',
        operation: 'collect',
        message: 'Timed out waiting for collection.',
      },
    })
    activity.groupedByDay.set([])

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    expect(screen.getByText('No activity loaded')).toBeTruthy()
  })

  it('offers to clear a search that matches nothing', async () => {
    const { activity } = mockActivity({ query: 'nope' })
    activity.groupedByDay.set([])

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    // The search field's own clear button shares the label; use the empty-state action.
    const [, emptyStateAction] = screen.getAllByRole('button', { name: 'Clear search' })
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
})
