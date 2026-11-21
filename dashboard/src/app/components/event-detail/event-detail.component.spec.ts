import { computed, type Provider, type Signal, signal, type WritableSignal } from '@angular/core'
import { provideRouter } from '@angular/router'
import { fireEvent, render, screen, within } from '@testing-library/angular'
import { beforeEach, describe, expect, it, vi } from 'vitest'
import type { EntryKey } from '../../providers/connection'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { createEntryAdapter } from '../../providers/provider-adapter'
import type { EntryUiAdapter } from '../../providers/types'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection } from '../../services/activity-selection.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { stubLayoutObservers } from '../../testing/observers'
import {
  MockBackendTransport,
  activityLog,
  provideMockBackend,
} from '../../testing/activity-fixtures'
import { EventDetailComponent } from './event-detail.component'

interface OpenDetails {
  readonly event?: EntryKey | null
  readonly events?: readonly EntryKey[]
}

/**
 * The parts of the activity services the drawer uses (the URL's state, the collection, the
 * open entries), with the URL as two signals: opening, stepping and closing set them as the real
 * navigations would. One object stands for all three services, each reading its own members.
 */
interface FakeActivity {
  readonly openEventKey: WritableSignal<EntryKey | null>
  readonly openGroupKeys: WritableSignal<readonly EntryKey[]>
  readonly detailsOpen: Signal<boolean>
  readonly dateFrom: Signal<string>
  readonly dateTo: Signal<string>
  readonly isLoading: WritableSignal<boolean>
  readonly meta: Signal<unknown>
  readonly hasNoSources: Signal<boolean>
  readonly collectionStatus: Signal<{ readonly kind: 'idle' }>
  readonly selectedEntry: Signal<EntryUiAdapter | null>
  readonly selectedEntries: Signal<readonly EntryUiAdapter[] | null>
  showInGroup(key: EntryKey): void
  stepInGroup(key: EntryKey): void
  backToGroupList(): void
  closeDetail(): void
}

/** Where the settings show, which decides the layer on top (`TopLayer`). */
interface FakeSettings {
  readonly dialogOpen: WritableSignal<boolean>
  readonly pageOpen: WritableSignal<boolean>
}

function fakeActivity(entries: readonly EntryUiAdapter[], open: OpenDetails): FakeActivity {
  const openEventKey = signal<EntryKey | null>(open.event ?? null)
  const openGroupKeys = signal<readonly EntryKey[]>(open.events ?? [])
  const byKey = new Map(entries.map(entry => [entry.trackKey, entry]))
  const isLoading = signal(false)
  return {
    openEventKey,
    openGroupKeys,
    detailsOpen: computed(() => openEventKey() !== null || openGroupKeys().length > 0),
    // The collected range: the URL names none.
    dateFrom: signal('2026-11-01'),
    dateTo: signal('2026-11-30'),
    isLoading,
    meta: signal(activityLog.meta),
    hasNoSources: signal(false),
    collectionStatus: signal({ kind: 'idle' as const }),
    selectedEntry: computed(() => {
      const key = openEventKey()
      return key === null ? null : (byKey.get(key) ?? null)
    }),
    selectedEntries: computed(() => {
      const keys = openGroupKeys()
      return keys.length === 0 ? null : keys.flatMap(key => byKey.get(key) ?? [])
    }),
    showInGroup: (key: EntryKey): void => openEventKey.set(key),
    stepInGroup: (key: EntryKey): void => openEventKey.set(key),
    backToGroupList: (): void => openEventKey.set(null),
    closeDetail: (): void => {
      openEventKey.set(null)
      openGroupKeys.set([])
    },
  }
}

async function renderDetails(
  entries: readonly EntryUiAdapter[],
  open: OpenDetails,
  providers: Provider[] = []
): Promise<{
  activity: FakeActivity
  settings: FakeSettings
  settle: () => Promise<void>
}> {
  const activity = fakeActivity(entries, open)
  const settings: FakeSettings = { dialogOpen: signal(false), pageOpen: signal(false) }
  const { fixture } = await render(EventDetailComponent, {
    providers: [
      provideRouter([]),
      ...providers,
      { provide: ActivityViewStateService, useValue: activity },
      { provide: ActivityDataService, useValue: activity },
      { provide: ActivitySelection, useValue: activity },
      { provide: SettingsDialogLauncher, useValue: settings },
    ],
  })
  return { activity, settings, settle: () => fixture.whenStable() }
}

describe('EventDetailComponent', () => {
  // The description measures its clamp with a `ResizeObserver`, which jsdom lacks.
  beforeEach(() => stubLayoutObservers())

  it('renders selected entry details as a named modal dialog', async () => {
    const entry = createEntryAdapter(activityLog.entries[0])
    await renderDetails([entry], { event: entry.trackKey })

    expect(screen.getByRole('dialog', { name: entry.title }).getAttribute('aria-modal')).toBe(
      'true'
    )
    expect(screen.getAllByRole('button', { name: /close event details/i })).toHaveLength(2)
  })

  it('shares a link that names the range shown, also when the URL names none', async () => {
    const copied = vi.fn<(text: string) => Promise<void>>(() => Promise.resolve())
    vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText: copied } })
    try {
      const entry = createEntryAdapter(activityLog.entries[0])
      await renderDetails([entry], { event: entry.trackKey })

      fireEvent.click(screen.getByRole('button', { name: /share/i }))

      await vi.waitFor(() => expect(copied).toHaveBeenCalledOnce())
      const link = new URL(copied.mock.calls[0]?.[0] ?? '')
      // In the browser the link is this page's own address.
      expect(link.origin).toBe(document.location.origin)
      expect(link.searchParams.get('from')).toBe('2026-11-01')
      expect(link.searchParams.get('to')).toBe('2026-11-30')
      expect(link.searchParams.get('event')).toBe(entry.trackKey)
    } finally {
      vi.unstubAllGlobals()
    }
  })

  it('shows the busy status of a meeting', async () => {
    const entry = createEntryAdapter({
      source: 'outlook',
      connection: 'default',
      id: 'offsite',
      timestamp: '2026-11-17T09:00:00Z',
      title: 'Offsite',
      isOrganizer: false,
      durationMinutes: 60,
      busyStatus: 'working_elsewhere',
      meetingWeight: 'busy',
    })

    await renderDetails([entry], { event: entry.trackKey })

    expect(screen.getByText('Status').nextElementSibling?.textContent).toBe('Working elsewhere')
  })

  it('shows the role only when Outlook said it, never an unknown one as attendee', async () => {
    const meeting = {
      source: 'outlook' as const,
      connection: 'default',
      timestamp: '2026-11-17T09:00:00Z',
      title: 'Planning',
      organizer: 'Alex',
      durationMinutes: 30,
      meetingWeight: 'busy' as const,
    }
    const attended = createEntryAdapter({ ...meeting, id: 'attended', isOrganizer: false })
    const unknown = createEntryAdapter({ ...meeting, id: 'unknown' })

    const { activity } = await renderDetails([attended, unknown], { event: attended.trackKey })
    expect(screen.getByText('Role').nextElementSibling?.textContent).toBe('Attendee')

    activity.openEventKey.set(unknown.trackKey)
    await vi.waitFor(() => expect(screen.queryByText('Role')).toBeNull())
    expect(screen.getByText('Organizer').nextElementSibling?.textContent).toBe('Alex')
    expect(unknown.meta).not.toMatch(/attendee|organizer/)
  })

  it('closes on Escape', async () => {
    const entry = createEntryAdapter(activityLog.entries[0])
    const { activity } = await renderDetails([entry], { event: entry.trackKey })

    fireEvent.keyDown(screen.getByRole('dialog', { name: entry.title }), { key: 'Escape' })

    expect(activity.openEventKey()).toBeNull()
  })

  it('leaves Escape to the settings dialog open over it, and to a dialog that handled it', async () => {
    const entry = createEntryAdapter(activityLog.entries[0])
    const { activity, settings } = await renderDetails([entry], { event: entry.trackKey })
    const drawer = screen.getByRole('dialog', { name: entry.title })

    settings.dialogOpen.set(true)
    fireEvent.keyDown(drawer, { key: 'Escape' })
    expect(activity.openEventKey()).toBe(entry.trackKey)

    settings.dialogOpen.set(false)
    const handled = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    handled.preventDefault()
    drawer.dispatchEvent(handled)
    expect(activity.openEventKey()).toBe(entry.trackKey)
  })

  it('joins an online meeting and links the description through the backend', async () => {
    const backend = new MockBackendTransport()
    const entry = createEntryAdapter({
      source: 'calendar',
      connection: 'default',
      id: 'planning',
      timestamp: '2026-11-17T09:00:00Z',
      title: 'Planning',
      durationMinutes: 30,
      meetingWeight: 'busy',
      description: 'Agenda: https://example.org/agenda\n\n<b>not markup</b>',
      descriptionBlocks: [
        {
          kind: 'paragraph',
          segments: [
            { kind: 'text', text: 'Agenda: ' },
            {
              kind: 'link',
              text: 'https://example.org/agenda',
              href: 'https://example.org/agenda',
            },
          ],
        },
        { kind: 'paragraph', segments: [{ kind: 'text', text: '<b>not markup</b>' }] },
      ],
      joinUrl: 'https://teams.microsoft.com/l/meetup-join/abc',
    })

    await renderDetails([entry], { event: entry.trackKey }, [provideMockBackend(backend)])

    const join = screen.getByRole('link', { name: /join meeting/i })
    expect(join.getAttribute('href')).toBe('https://teams.microsoft.com/l/meetup-join/abc')
    expect(join.getAttribute('rel')).toBe('noopener noreferrer')
    fireEvent.click(join)

    const description = screen.getByRole('region', { name: 'Description' })
    expect(description.querySelector('b')).toBeNull()
    expect(description.textContent).toContain('<b>not markup</b>')
    const link = screen.getByRole('link', { name: 'https://example.org/agenda' })
    fireEvent.click(link)

    expect(backend.openedUrls).toEqual([
      'https://teams.microsoft.com/l/meetup-join/abc',
      'https://example.org/agenda',
    ])
  })

  it('shows a description that is only an image', async () => {
    const entry = createEntryAdapter({
      source: 'calendar',
      connection: 'default',
      id: 'banner',
      timestamp: '2026-11-17T09:00:00Z',
      title: 'Kickoff',
      durationMinutes: 30,
      meetingWeight: 'busy',
      descriptionBlocks: [{ kind: 'image', src: 'https://example.org/banner.png' }],
    })

    await renderDetails([entry], { event: entry.trackKey })

    const description = screen.getByRole('region', { name: 'Description' })
    expect(description.querySelector('img')?.getAttribute('src')).toBe(
      'https://example.org/banner.png'
    )
  })

  describe('overlapping events', () => {
    const meeting = (title: string, joinUrl?: string): EntryUiAdapter =>
      createEntryAdapter({
        source: 'outlook',
        connection: 'default',
        id: title,
        timestamp: '2026-11-17T09:00:00Z',
        title,
        isOrganizer: false,
        durationMinutes: 30,
        meetingWeight: 'busy',
        description: `About ${title}`,
        descriptionBlocks: [
          { kind: 'paragraph', segments: [{ kind: 'text', text: `About ${title}` }] },
        ],
        joinUrl,
      })

    interface Group {
      readonly backend: MockBackendTransport
      readonly activity: FakeActivity
      readonly settle: () => Promise<void>
    }

    async function renderGroup(): Promise<Group> {
      const backend = new MockBackendTransport()
      const entries = [meeting('Sync', 'https://zoom.us/j/1'), meeting('Lunch'), meeting('Review')]
      const { activity, settle } = await renderDetails(
        entries,
        { events: entries.map(entry => entry.trackKey) },
        [provideMockBackend(backend)]
      )
      return { backend, activity, settle }
    }

    const dialog = (name: string | RegExp): HTMLElement => screen.getByRole('dialog', { name })
    const position = (): string | undefined =>
      screen.getByRole('status').textContent?.replace(/\s+/g, ' ').trim()

    it('opens as a list with one row per event and no details', async () => {
      await renderGroup()

      const list = dialog('3 overlapping events')
      const rows = within(list).getAllByRole('listitem')
      expect(rows.map(row => within(row).getByRole('button').textContent)).toEqual([
        expect.stringContaining('Sync'),
        expect.stringContaining('Lunch'),
        expect.stringContaining('Review'),
      ])
      expect(within(rows[0]).getByRole('button').textContent).toContain('Online meeting')
      expect(screen.queryByRole('link', { name: /join/i })).toBeNull()
      expect(screen.queryByRole('region', { name: 'Description' })).toBeNull()
    })

    it('opens a row into the same details as a single event, with its position', async () => {
      const { backend, settle } = await renderGroup()

      fireEvent.click(screen.getByRole('button', { name: /Sync/ }))
      await settle()

      const detail = dialog('Sync')
      expect(position()).toBe('1 of 3: Sync')
      expect(document.activeElement).toBe(within(detail).getByRole('heading', { name: 'Sync' }))
      expect(screen.getByRole('region', { name: 'Description' }).textContent).toContain(
        'About Sync'
      )
      fireEvent.click(screen.getByRole('link', { name: /join meeting/i }))
      expect(backend.openedUrls).toEqual(['https://zoom.us/j/1'])
    })

    it('steps through the group with the buttons and the arrow keys', async () => {
      const { settle } = await renderGroup()
      fireEvent.click(screen.getByRole('button', { name: /Lunch/ }))
      await settle()

      fireEvent.click(screen.getByRole('button', { name: 'Next event' }))
      await settle()
      expect(dialog('Review')).toBeTruthy()
      expect(position()).toBe('3 of 3: Review')
      expect(screen.getByRole('button', { name: 'Next event' }).getAttribute('aria-disabled')).toBe(
        'true'
      )

      const drawer = dialog('Review')
      fireEvent.keyDown(drawer, { key: 'ArrowUp' })
      await settle()
      fireEvent.keyDown(drawer, { key: 'ArrowUp' })
      await settle()
      expect(dialog('Sync')).toBeTruthy()
      expect(
        screen.getByRole('button', { name: 'Previous event' }).getAttribute('aria-disabled')
      ).toBe('true')
      // At the first event there is nothing to step to: the key is left alone.
      expect(fireEvent.keyDown(dialog('Sync'), { key: 'ArrowUp' })).toBe(true)
      expect(position()).toBe('1 of 3: Sync')

      fireEvent.keyDown(dialog('Sync'), { key: 'ArrowDown' })
      await settle()
      expect(position()).toBe('2 of 3: Lunch')
    })

    it('keeps focus on the stepper when it reaches the last event', async () => {
      const { settle } = await renderGroup()
      fireEvent.click(screen.getByRole('button', { name: /Lunch/ }))
      await settle()

      const next = screen.getByRole('button', { name: 'Next event' }) as HTMLButtonElement
      next.focus()
      fireEvent.click(next)
      await settle()

      expect(position()).toBe('3 of 3: Review')
      expect(next.disabled).toBe(false)
      expect(document.activeElement).toBe(next)
      fireEvent.click(next)
      await settle()
      expect(position()).toBe('3 of 3: Review')
    })

    it('closes on Escape when a click left focus on nothing', async () => {
      const { activity, settle } = await renderGroup()
      fireEvent.click(screen.getByRole('button', { name: /Review/ }))
      await settle()
      ;(document.activeElement as HTMLElement | null)?.blur()

      fireEvent.keyDown(document.body, { key: 'Escape' })

      expect(activity.openGroupKeys()).toEqual([])
    })

    it('goes back to the list and focuses the row it came from', async () => {
      const { settle } = await renderGroup()
      fireEvent.click(screen.getByRole('button', { name: /Lunch/ }))
      await settle()

      fireEvent.click(screen.getByRole('button', { name: '3 overlapping events' }))
      await settle()

      expect(dialog('3 overlapping events')).toBeTruthy()
      expect(document.activeElement).toBe(screen.getByRole('button', { name: /Lunch/ }))
    })

    it('focuses the details title when the open event leaves the group', async () => {
      const { activity, settle } = await renderGroup()
      fireEvent.click(screen.getByRole('button', { name: /Lunch/ }))
      await settle()

      screen.getByRole('button', { name: 'Next event' }).focus()
      // The stepper goes with the group position.
      activity.openGroupKeys.update(keys => keys.filter(key => key !== activity.openEventKey()))
      await settle()

      expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Lunch' }))
    })

    it('closes from the details on Escape', async () => {
      const { activity, settle } = await renderGroup()
      fireEvent.click(screen.getByRole('button', { name: /Review/ }))
      await settle()

      fireEvent.keyDown(dialog('Review'), { key: 'Escape' })

      expect(activity.openGroupKeys()).toEqual([])
      expect(activity.openEventKey()).toBeNull()
    })
  })

  it('shows no meeting block for an entry without description or join link', async () => {
    const entry = createEntryAdapter(activityLog.entries[0])
    await renderDetails([entry], { event: entry.trackKey })

    expect(screen.queryByRole('link', { name: /join meeting/i })).toBeNull()
    expect(screen.queryByRole('region', { name: 'Description' })).toBeNull()
  })

  describe('a linked event that is not loaded', () => {
    it('shows a loading state while the log is still coming in', async () => {
      const { activity, settle } = await renderDetails([], { event: 'git/work/1' })
      activity.isLoading.set(true)
      await settle()

      expect(screen.getByRole('dialog', { name: 'Loading the event' })).toBeTruthy()
    })

    it('says so once the collection is in without it, and closes from there', async () => {
      const { activity } = await renderDetails([], { event: 'git/work/1' })

      const dialog = screen.getByRole('dialog', { name: 'Event not found' })
      expect(
        within(dialog).getByRole('heading', { name: 'This event is not in the collected range' })
      ).toBeTruthy()
      fireEvent.click(within(dialog).getByRole('button', { name: 'Close event details' }))
      expect(activity.openEventKey()).toBeNull()
    })
  })
})
