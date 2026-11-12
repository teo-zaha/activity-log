import { signal, type WritableSignal } from '@angular/core'
import type { Source } from '../../generated/activity'
import type { DayGroup } from '../../models/day-group'
import type { EntryUiAdapter } from '../../providers/types'
import type { CollectionStatus } from '../../services/types'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { createEntryAdapter } from '../../providers/provider-adapter'
import { ActivityService } from '../../services/activity.service'
import { activityLog } from '../../testing/activity-fixtures'
import { TimelineComponent } from './timeline.component'

interface TimelineActivityMock {
  isLoading: WritableSignal<boolean>
  groupedByDay: WritableSignal<DayGroup<EntryUiAdapter>[]>
  searchQuery: WritableSignal<string>
  collectionStatus: WritableSignal<CollectionStatus>
  providers: WritableSignal<{ label: string }[]>
  activeSources: WritableSignal<Set<Source>>
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
      collectionStatus: signal<CollectionStatus>(overrides.status ?? { kind: 'idle' }),
      providers: signal([{ label: 'Git' }]),
      activeSources: signal(new Set<Source>(['git'])),
      selectedEntries: signal<EntryUiAdapter[] | null>(null),
      selectedEntry: signal<EntryUiAdapter | null>(null),
    },
  }
}

describe('TimelineComponent', () => {
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
})
