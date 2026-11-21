import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { describe, expect, it } from 'vitest'
import type { ActivityEntry } from '../generated/activity'
import { provideAppRouter } from '../app.routes'
import {
  activityLog,
  MockBackendTransport,
  providerInfo,
  provideMockBackend,
} from '../testing/activity-fixtures'
import { ActivityDataService } from './activity-data.service'
import { ActivitySelection, type RangeCoverage } from './activity-selection.service'
import { ActivityViewStateService, type DayRange } from './activity-view-state.service'

/** 16 May 22:00 to 17 May 02:00 local; the collected range starts on the 17th. */
const overnight: ActivityEntry = {
  source: 'calendar',
  connection: 'team',
  id: 'night-shift',
  timestamp: new Date(2026, 4, 16, 22).toISOString(),
  title: 'Night shift',
  durationMinutes: 240,
  meetingWeight: 'busy',
}

/** The selection over the fixture's log of 17–18 May, loaded, with the overnight meeting in it. */
async function loaded(): Promise<{ selection: ActivitySelection; router: Router }> {
  const backend = new MockBackendTransport()
  backend.providersResponse = {
    currentOs: 'linux',
    providers: [
      ...providerInfo,
      { ...providerInfo[0]!, connection: { source: 'calendar', name: 'team' }, label: 'Team' },
    ],
  }
  backend.activityResponses = []
  backend.activityResponse = { ...activityLog, entries: [...activityLog.entries, overnight] }
  TestBed.configureTestingModule({ providers: [provideMockBackend(backend), provideAppRouter()] })
  await TestBed.inject(ActivityDataService).initLoad()
  return { selection: TestBed.inject(ActivitySelection), router: TestBed.inject(Router) }
}

const ids = (entries: readonly { readonly entry: ActivityEntry }[]): string[] =>
  entries.map(({ entry }) => entry.id)

describe('ActivitySelection', () => {
  it('lists the shown sources’ entries of the range by the day they start, filtered on the Timeline', async () => {
    const { selection, router } = await loaded()

    await router.navigateByUrl('/timeline')
    // The night shift starts on the 16th, before the range.
    expect(ids(selection.filteredEntries())).toEqual(['abcdef123456', 'AL-1:updated'])
    expect(selection.groupedByDay().map(group => group.date)).toEqual(['2026-05-18'])

    await router.navigateByUrl('/timeline?q=DASHBOARD')
    expect(ids(selection.filteredEntries())).toEqual(['abcdef123456'])
  })

  it('draws a meeting that runs into the range in the calendar views, without the filter', async () => {
    const { selection, router } = await loaded()

    await router.navigateByUrl('/week/2026-05-18?q=dashboard')

    expect(ids(selection.calendarEntries())).toEqual([
      'abcdef123456',
      'AL-1:updated',
      'night-shift',
    ])
  })

  it('exports the Timeline’s list or what a calendar view draws', async () => {
    const { selection, router } = await loaded()

    await router.navigateByUrl('/week/2026-05-18')
    expect(ids(selection.exportedEntries())).toContain('night-shift')
    expect(selection.exportedEntries()).toHaveLength(3)

    await router.navigateByUrl('/timeline')
    expect(ids(selection.exportedEntries())).not.toContain('night-shift')
  })

  it('shows, hides and isolates a source against the sources on screen', async () => {
    const { selection, router } = await loaded()
    await router.navigateByUrl('/timeline')

    selection.toggleConnection('git/default')
    expect(ids(selection.filteredEntries())).toEqual(['AL-1:updated'])

    selection.toggleConnection('git/default')
    selection.isolateConnection('git/default')
    expect(ids(selection.filteredEntries())).toEqual(['abcdef123456'])

    selection.isolateConnection('git/default')
    expect(selection.filteredEntries()).toHaveLength(2)

    // Not a source on screen: nothing changes.
    selection.toggleConnection('gitlab/default')
    expect(selection.filteredEntries()).toHaveLength(2)
  })

  it('reads the open event and the listed group from the URL, once their entries are in', async () => {
    const { selection, router } = await loaded()

    await router.navigateByUrl('/timeline')
    expect(selection.selectedEntry()).toBeNull()
    expect(selection.selectedEntries()).toBeNull()

    // A group whose second event is not in the log: the loaded one is listed.
    await router.navigateByUrl(
      '/timeline?events=git%2Fdefault%2Fabcdef123456&events=jira%2Fdefault%2Fgone' +
        '&event=git%2Fdefault%2Fabcdef123456'
    )
    expect(selection.selectedEntry()?.entry.id).toBe('abcdef123456')
    expect(ids(selection.selectedEntries() ?? [])).toEqual(['abcdef123456'])

    await router.navigateByUrl('/timeline?event=jira%2Fdefault%2FAL-1:updated')
    expect(selection.selectedEntry()?.entry.id).toBe('AL-1:updated')
    expect(selection.selectedEntries()).toBeNull()

    await router.navigateByUrl('/timeline?event=git%2Fdefault%2Fgone')
    expect(selection.selectedEntry()).toBeNull()
  })

  describe('rangeCoverage', () => {
    const MAY: DayRange = { from: '2026-05-10', to: '2026-05-20' }

    async function coverage(
      own: DayRange | null,
      collected: DayRange | null
    ): Promise<RangeCoverage> {
      TestBed.configureTestingModule({
        providers: [provideMockBackend(new MockBackendTransport()), provideAppRouter()],
      })
      TestBed.inject(ActivityViewStateService).showCollectedRange(collected)
      await TestBed.inject(Router).navigateByUrl(
        own === null ? '/timeline' : `/timeline?from=${own.from}&to=${own.to}`
      )
      return TestBed.inject(ActivitySelection).rangeCoverage()
    }

    it.each<{
      name: string
      own: DayRange | null
      collected: DayRange | null
      expected: RangeCoverage
    }>([
      { name: 'nothing collected', own: MAY, collected: null, expected: { kind: 'unknown' } },
      // Without a range of its own the tab shows the collected one.
      { name: 'no range of its own', own: null, collected: MAY, expected: { kind: 'covered' } },
      { name: 'no range at all', own: null, collected: null, expected: { kind: 'unknown' } },
      { name: 'the same range', own: MAY, collected: MAY, expected: { kind: 'covered' } },
      {
        name: 'a range inside the collected one',
        own: { from: '2026-05-12', to: '2026-05-14' },
        collected: MAY,
        expected: { kind: 'covered' },
      },
      {
        name: 'a range that overlaps the collected one',
        own: { from: '2026-05-15', to: '2026-05-25' },
        collected: MAY,
        expected: {
          kind: 'other',
          collected: MAY,
          own: { from: '2026-05-15', to: '2026-05-25' },
          overlaps: true,
        },
      },
      {
        name: 'a range that ends on the collected one’s first day',
        own: { from: '2026-05-01', to: '2026-05-10' },
        collected: MAY,
        expected: {
          kind: 'other',
          collected: MAY,
          own: { from: '2026-05-01', to: '2026-05-10' },
          overlaps: true,
        },
      },
      {
        name: 'a range outside the collected one',
        own: { from: '2026-04-01', to: '2026-04-08' },
        collected: MAY,
        expected: {
          kind: 'other',
          collected: MAY,
          own: { from: '2026-04-01', to: '2026-04-08' },
          overlaps: false,
        },
      },
      {
        name: 'a range around the collected one',
        own: { from: '2026-05-01', to: '2026-05-31' },
        collected: MAY,
        expected: {
          kind: 'other',
          collected: MAY,
          own: { from: '2026-05-01', to: '2026-05-31' },
          overlaps: true,
        },
      },
    ])('$name', async ({ own, collected, expected }) => {
      expect(await coverage(own, collected)).toEqual(expected)
    })
  })
})
