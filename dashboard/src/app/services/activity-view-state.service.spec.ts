import { Location } from '@angular/common'
import { provideLocationMocks, SpyLocation } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { provideAppRouter } from '../app.routes'
import { connectionSet, gitlabProvider as gitlab, providerInfo } from '../testing/activity-fixtures'
import { ActivityViewStateService } from './activity-view-state.service'

describe('ActivityViewStateService', () => {
  it('toggles active sources only for usable providers', () => {
    const service = TestBed.inject(ActivityViewStateService)
    service.activeConnections.set(connectionSet('git', 'jira'))

    service.toggleConnection('git/default', providerInfo)

    expect(service.activeConnections().has('git/default')).toBe(false)
    expect(service.activeConnections().has('jira/default')).toBe(true)
  })

  it('isolates a source and then restores all usable providers', () => {
    const service = TestBed.inject(ActivityViewStateService)
    service.activeConnections.set(connectionSet('git', 'jira'))

    service.isolateConnection('git/default', providerInfo)
    expect(Array.from(service.activeConnections())).toEqual(['git/default'])

    service.isolateConnection('git/default', providerInfo)
    expect(service.activeConnections()).toEqual(connectionSet('git', 'jira'))
  })

  it('shows a source while it is checked and drops it when its check fails', () => {
    const service = TestBed.inject(ActivityViewStateService)
    const [git, jira] = providerInfo
    if (!git || !jira) throw new Error('fixture has two providers')
    const checking = { ...jira, status: 'checking', usable: false } as const

    service.activateAllUsable([git, checking])
    expect(service.activeConnections()).toEqual(connectionSet('git', 'jira'))

    service.followProviders([git, checking], [git, checking])
    expect(service.activeConnections()).toEqual(connectionSet('git', 'jira'))

    const failed = { ...jira, status: 'unavailable', usable: false } as const
    service.followProviders([git, failed], [git, checking])
    expect(service.activeConnections()).toEqual(connectionSet('git'))

    // Usable again: shown again, like a source that is new.
    service.followProviders([git, jira], [git, failed])
    expect(service.activeConnections()).toEqual(connectionSet('git', 'jira'))
  })

  it('shows a new source and leaves switched-off sources off', () => {
    const service = TestBed.inject(ActivityViewStateService)
    const [git, jira] = providerInfo
    if (!git || !jira) throw new Error('fixture has two providers')
    service.activeConnections.set(connectionSet('jira'))

    service.followProviders([git, jira, gitlab], [git, jira])

    expect(service.activeConnections()).toEqual(connectionSet('jira', 'gitlab'))
  })
})

describe('ActivityViewStateService URL state', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date(2026, 10, 17, 12))
  })
  afterEach(() => vi.useRealTimers())

  async function open(url: string): Promise<{
    service: ActivityViewStateService
    router: Router
    location: SpyLocation
  }> {
    TestBed.configureTestingModule({ providers: [provideAppRouter(), provideLocationMocks()] })
    const router = TestBed.inject(Router)
    const location = TestBed.inject(Location)
    if (!(location instanceof SpyLocation)) throw new Error('location mocks are provided')
    router.initialNavigation()
    await router.navigateByUrl(url)
    location.urlChanges = []
    return { service: TestBed.inject(ActivityViewStateService), router, location }
  }

  describe('event details', () => {
    const query = (router: Router): Record<string, unknown> =>
      router.parseUrl(router.url).queryParams

    it('opens an event over the view and closes it with Back', async () => {
      const { service, router, location } = await open('/day/2026-11-17?q=x')

      await service.openEvent('outlook/work/a/1')
      expect(query(router)).toEqual({ q: 'x', event: 'outlook/work/a/1' })
      expect(service.openEventKey()).toBe('outlook/work/a/1')

      location.back()
      await vi.waitFor(() => expect(router.url).toBe('/day/2026-11-17?q=x'))
      expect(service.openEventKey()).toBeNull()
    })

    it('pushes a row of a group, steps in place, and closes past all of it', async () => {
      const { service, router, location } = await open('/day/2026-11-17')
      await service.openGroup(['git/a/1', 'git/a/2', 'git/a/3'])
      await service.showInGroup('git/a/1')
      await service.stepInGroup('git/a/2')

      expect(service.openGroupKeys()).toEqual(['git/a/1', 'git/a/2', 'git/a/3'])
      expect(service.openEventKey()).toBe('git/a/2')
      // List and row are history entries; the step replaced the row.
      expect(location.urlChanges).toHaveLength(3)
      expect(location.urlChanges[2]).toMatch(/^replace: /)

      await service.backToGroupList()
      await vi.waitFor(() => expect(service.openEventKey()).toBeNull())
      expect(service.openGroupKeys()).toHaveLength(3)

      await service.showInGroup('git/a/3')
      await service.closeDetail()
      await vi.waitFor(() => expect(router.url).toBe('/day/2026-11-17'))
    })

    it('restores details from a reloaded URL and closes them in place', async () => {
      const { service, router, location } = await open(
        '/timeline?events=git%2Fa%2F1&events=git%2Fa%2F2&event=git%2Fa%2F2'
      )
      expect(service.openEventKey()).toBe('git/a/2')
      expect(service.openGroupKeys()).toEqual(['git/a/1', 'git/a/2'])

      // Nothing to go back to: the list replaces the event, Close replaces the list.
      await service.backToGroupList()
      expect(query(router)).toEqual({ events: ['git/a/1', 'git/a/2'] })
      await service.closeDetail()
      expect(router.url).toBe('/timeline')
      expect(location.urlChanges.every(change => change.startsWith('replace: '))).toBe(true)
    })

    it('closes a row of a linked group past the link, leaving no details open', async () => {
      const { service, router } = await open(
        '/day/2026-11-17?events=git%2Fa%2F1&events=git%2Fa%2F2'
      )
      await service.showInGroup('git/a/1')

      await service.closeDetail()
      expect(router.url).toBe('/day/2026-11-17')
      expect(service.openGroupKeys()).toEqual([])
    })

    it('closes a linked group after going from a row back to it', async () => {
      const { service, router } = await open(
        '/day/2026-11-17?events=git%2Fa%2F1&events=git%2Fa%2F2'
      )
      await service.showInGroup('git/a/1')
      await service.backToGroupList()
      expect(service.openEventKey()).toBeNull()

      await service.closeDetail()
      expect(router.url).toBe('/day/2026-11-17')
    })

    it('goes back once for a double close', async () => {
      const { service, router } = await open('/day/2026-11-16')
      await router.navigateByUrl('/day/2026-11-17')
      await service.openEvent('git/a/1')

      const first = service.closeDetail()
      const second = service.closeDetail()
      expect(second).toBe(first)
      await first
      expect(router.url).toBe('/day/2026-11-17')
    })
  })

  it('reads the view, its day, the filter, the range and the settings from the URL', async () => {
    const { service } = await open(
      '/week/2026-11-10?q=review&from=2026-11-01&to=2026-11-14&settings=gitlab%2Foss'
    )

    expect(service.currentView()).toBe('week')
    expect(service.selectedDay()).toBe('2026-11-10')
    expect(service.weekDays()[0]).toBe('2026-11-09')
    expect(service.searchQuery()).toBe('review')
    expect(service.dateFrom()).toBe('2026-11-01')
    expect(service.dateTo()).toBe('2026-11-14')
    expect(service.settings()).toBe('connection:gitlab/oss')
  })

  it('shows the timeline for / and unknown paths, keeping the query', async () => {
    const { router } = await open('/?settings=logs')
    expect(router.url).toBe('/timeline?settings=logs')

    await router.navigateByUrl('/nowhere/at/all?q=x')
    expect(router.url).toBe('/timeline?q=x')
  })

  it('shows today for a missing or invalid day', async () => {
    const { router, service } = await open('/day/2026-02-30?from=2026-02-01&to=2026-02-28')
    expect(router.url).toBe('/day/2026-11-17?from=2026-02-01&to=2026-02-28')
    expect(service.selectedDay()).toBe('2026-11-17')

    await router.navigateByUrl('/workweek')
    expect(router.url).toBe('/workweek/2026-11-17')
    expect(service.currentView()).toBe('workweek')
  })

  it('ignores invalid query values', async () => {
    const { service } = await open('/timeline?from=yesterday&to=2026-13-01&settings=nope%2Fx')

    expect(service.dateFrom()).toBe('')
    expect(service.dateTo()).toBe('')
    expect(service.settings()).toBeNull()
  })

  it('falls back to the collected range while the URL names none', async () => {
    const { service, router } = await open('/timeline')
    service.showCollectedRange({ from: '2026-11-10', to: '2026-11-16' })
    expect([service.dateFrom(), service.dateTo()]).toEqual(['2026-11-10', '2026-11-16'])

    await router.navigateByUrl('/timeline?from=2026-11-01&to=2026-11-02')
    expect([service.dateFrom(), service.dateTo()]).toEqual(['2026-11-01', '2026-11-02'])
  })

  it('pushes a history entry for a view or day and drops the filter on a view switch', async () => {
    const { service, router, location } = await open(
      '/timeline?q=ticket&from=2026-11-01&to=2026-11-14'
    )

    await service.setView('day')
    expect(router.url).toBe('/day/2026-11-17?from=2026-11-01&to=2026-11-14')
    expect(service.searchQuery()).toBe('')

    service.navigateDay(-1)
    await vi.waitFor(() => expect(service.selectedDay()).toBe('2026-11-16'))
    expect(router.url).toBe('/day/2026-11-16?from=2026-11-01&to=2026-11-14')
    expect(location.urlChanges).toEqual([
      '/day/2026-11-17?from=2026-11-01&to=2026-11-14',
      '/day/2026-11-16?from=2026-11-01&to=2026-11-14',
    ])
  })

  it('keeps the day the calendar views showed last while the timeline is open', async () => {
    const { service } = await open('/week/2026-11-03')

    await service.setView('list')
    expect(service.currentView()).toBe('list')
    expect(service.selectedDay()).toBe('2026-11-03')

    await service.setView('day')
    expect(service.selectedDay()).toBe('2026-11-03')
  })

  it('filters at once and puts the filter in the URL once typing pauses', async () => {
    const { service, router, location } = await open('/timeline')

    service.setSearchQuery('rev')
    service.setSearchQuery('review')
    expect(service.searchQuery()).toBe('review')
    expect(router.url).toBe('/timeline')

    // A range change meanwhile keeps the typed text, which follows into the URL.
    await service.setDateFrom('2026-11-02')
    await service.setDateTo('2026-11-09')
    expect(service.searchQuery()).toBe('review')

    await vi.waitFor(() =>
      expect(router.url).toBe('/timeline?from=2026-11-02&to=2026-11-09&q=review')
    )
    expect(service.searchQuery()).toBe('review')
    // One entry for the filter, and the typing and ranges replace the current entry.
    expect(location.urlChanges.filter(change => change.includes('q='))).toHaveLength(1)
    expect(location.urlChanges.every(change => change.startsWith('replace: '))).toBe(true)

    service.setSearchQuery('')
    expect(service.searchQuery()).toBe('')
    await vi.waitFor(() => expect(router.url).toBe('/timeline?from=2026-11-02&to=2026-11-09'))
  })

  it('drops filter text not yet in the URL when the view switches', async () => {
    const { service, router } = await open('/timeline')

    service.setSearchQuery('review')
    await service.setView('day')
    expect(service.searchQuery()).toBe('')

    // The pending URL update finds nothing left to write.
    await new Promise(resolve => setTimeout(resolve, 250))
    expect(router.url).toBe('/day/2026-11-17')
  })

  it('opens the day view on a day', async () => {
    const { service, router } = await open('/week/2026-11-10')

    await service.openDay('2026-11-12')

    expect(router.url).toBe('/day/2026-11-12')
  })

  it('opens the settings as a history entry, moves between sections in place and closes with Back', async () => {
    const { service, router, location } = await open('/day/2026-11-17')

    await service.openSettings('connection:jira/default')
    expect(router.url).toBe('/day/2026-11-17?settings=jira%2Fdefault')
    await service.showSettings('logs')
    expect(service.settings()).toBe('logs')
    expect(location.urlChanges).toEqual([
      '/day/2026-11-17?settings=jira%2Fdefault',
      'replace: /day/2026-11-17?settings=logs',
    ])

    await service.closeSettings()
    await vi.waitFor(() => expect(router.url).toBe('/day/2026-11-17'))
  })

  it('opens a section as a page from the settings home, and goes back past both to close', async () => {
    const { service, router, location } = await open('/timeline')

    await service.openSettings('home')
    await service.enterSettings('logs')
    expect(location.urlChanges).toEqual(['/timeline?settings=home', '/timeline?settings=logs'])

    await service.backToSettingsHome()
    await vi.waitFor(() => expect(service.settings()).toBe('home'))

    await service.enterSettings('add')
    await service.closeSettings()
    await vi.waitFor(() => expect(router.url).toBe('/timeline'))
  })

  it('goes from a linked settings section to the home list and closes in place', async () => {
    const { service, router, location } = await open('/timeline?settings=logs')

    await service.backToSettingsHome()
    expect(router.url).toBe('/timeline?settings=home')
    await service.closeSettings()
    expect(router.url).toBe('/timeline')
    expect(location.urlChanges.every(change => change.startsWith('replace: '))).toBe(true)
  })

  it('closes a section page entered from linked settings home, leaving no settings open', async () => {
    const { service, router } = await open('/timeline?settings=home')
    await service.enterSettings('logs')

    await service.closeSettings()
    expect(router.url).toBe('/timeline')
  })

  it('reads the range of the URL the dashboard was opened with', () => {
    TestBed.configureTestingModule({ providers: [provideAppRouter(), provideLocationMocks()] })
    const location = TestBed.inject(Location)
    location.go('/day/2026-11-17?from=2026-11-01&to=2026-11-14')
    const service = TestBed.inject(ActivityViewStateService)

    expect(service.openedRange()).toEqual({ from: '2026-11-01', to: '2026-11-14' })

    location.go('/day/2026-11-17?from=2026-11-01')
    expect(service.openedRange()).toBeNull()
  })
})
