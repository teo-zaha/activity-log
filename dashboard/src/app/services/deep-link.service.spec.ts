import { provideLocationMocks } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest'
import { provideAppRouter } from '../app.routes'
import { ActivityDataService } from './activity-data.service'
import { appLink } from '../components/share/share-link'
import { DeepLinkService } from './deep-link.service'

describe('DeepLinkService', () => {
  beforeEach(() => {
    vi.useFakeTimers({ toFake: ['Date'] })
    vi.setSystemTime(new Date(2026, 10, 17, 12))
  })
  afterEach(() => vi.useRealTimers())

  async function setup(): Promise<{
    links: DeepLinkService
    router: Router
    collect: ReturnType<typeof vi.fn>
  }> {
    const collect = vi.fn(() => Promise.resolve())
    TestBed.configureTestingModule({
      providers: [
        provideAppRouter(),
        provideLocationMocks(),
        { provide: ActivityDataService, useValue: { collect } },
      ],
    })
    const router = TestBed.inject(Router)
    await router.navigateByUrl('/week/2026-11-10?q=old&settings=logs')
    return { links: TestBed.inject(DeepLinkService), router, collect }
  }

  it('opens an activity-log: link as the route it names', async () => {
    const { links, router, collect } = await setup()

    expect(await links.open('activity-log://day/2026-11-16?q=review')).toBe(true)

    // The link's query replaces the current one: the settings close, the filter is the link's.
    expect(router.url).toBe('/day/2026-11-16?q=review')
    expect(collect).not.toHaveBeenCalled()
  })

  it('opens a plain path and collects the range it names', async () => {
    const { links, router, collect } = await setup()

    expect(await links.open('/timeline?from=2026-11-01&to=2026-11-14&settings=gitlab%2Foss')).toBe(
      true
    )

    expect(router.url).toBe('/timeline?from=2026-11-01&to=2026-11-14&settings=gitlab%2Foss')
    expect(collect).toHaveBeenCalledWith('2026-11-01', '2026-11-14')
  })

  it('opens the details a link names and drops event keys that name no entry', async () => {
    const { links, router } = await setup()

    await links.open('activity-log://day/2026-11-16?event=outlook/work/AAMk%2B1&events=bad')
    expect(router.parseUrl(router.url).queryParams).toEqual({ event: 'outlook/work/AAMk+1' })

    await links.open(
      'activity-log://day/2026-11-16?events=git/a/1&events=git/a/2&events=github/x/3&event=git/a/2'
    )
    expect(router.parseUrl(router.url).queryParams).toEqual({
      events: ['git/a/1', 'git/a/2'],
      event: 'git/a/2',
    })
  })

  it('shows today for a link without a valid day and drops unknown or invalid parameters', async () => {
    const { links, router } = await setup()

    await links.open('activity-log://week/someday?from=soon&utm_source=mail')

    expect(router.url).toBe('/week/2026-11-17')
  })

  it('drops a range the backend would not collect, and collects nothing for it', async () => {
    const { links, router, collect } = await setup()

    await links.open('activity-log://timeline?from=1000-01-01&to=9999-12-31&q=x')
    expect(router.url).toBe('/timeline?q=x')
    await links.open('activity-log://timeline?from=2026-11-14&to=2026-11-01')
    expect(router.url).toBe('/timeline')
    await links.open('activity-log://timeline?from=2016-11-18&to=2026-11-17')
    expect(router.url).toBe('/timeline?from=2016-11-18&to=2026-11-17')

    expect(collect.mock.calls).toEqual([['2016-11-18', '2026-11-17']])
  })

  describe('in Berlin, across a change to summer time', () => {
    beforeAll(() => vi.stubEnv('TZ', 'Europe/Berlin'))
    afterAll(() => vi.unstubAllEnvs())

    it('checks a link’s range as the backend does, in instants', async () => {
      // 30 March 2020 is summer time, 31 March 2030 00:00 is still winter time.
      expect(new Date(2020, 2, 30).getTimezoneOffset()).toBe(-120)
      expect(new Date(2030, 2, 31).getTimezoneOffset()).toBe(-60)
      const { links, router, collect } = await setup()

      // Ten years of days, inclusive, last one hour longer than 3653 days: refused.
      await links.open('activity-log://timeline?from=2020-03-30&to=2030-03-30')
      expect(router.url).toBe('/timeline')
      // A day less is 3652 days and one hour.
      await links.open('activity-log://timeline?from=2020-03-30&to=2030-03-29')
      expect(router.url).toBe('/timeline?from=2020-03-30&to=2030-03-29')

      expect(collect.mock.calls).toEqual([['2020-03-30', '2030-03-29']])
    })
  })

  it('opens a shared link to an event with / and # in its id as that event', async () => {
    const { links, router } = await setup()
    // A numbered id (`make_ids_unique`) holds `#`; ids also hold `/`.
    for (const key of ['git/work/1f3a#2', 'calendar/team/uid/1@example.org#2']) {
      await router.navigate(['/day', '2026-11-17'], { queryParams: { event: key } })
      const shared = appLink(router, ['event'])
      expect(shared).not.toContain('#')
      await router.navigateByUrl('/timeline')

      expect(await links.open(shared)).toBe(true)
      expect(router.parseUrl(router.url).queryParams['event']).toBe(key)
      // A reload reads the same address again.
      expect(
        router.parseUrl(router.serializeUrl(router.parseUrl(router.url))).queryParams['event']
      ).toBe(key)
    }
  })

  it('ignores a link to no view or another scheme', async () => {
    const { links, router } = await setup()

    expect(await links.open('activity-log://admin')).toBe(false)
    expect(await links.open('https://example.com/day/2026-11-17')).toBe(false)
    expect(await links.open('/day/2026-11-17/extra')).toBe(false)

    expect(router.url).toBe('/week/2026-11-10?q=old&settings=logs')
  })
})
