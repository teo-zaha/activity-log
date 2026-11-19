import { provideLocationMocks } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { describe, expect, it, type MockInstance, vi } from 'vitest'
import { provideAppRouter } from '../app.routes'
import { MockBackendTransport, provideMockBackend } from '../testing/activity-fixtures'
import { ActivityService } from './activity.service'
import { DeepLinkInboxService } from './deep-link-inbox.service'
import { DeepLinkService } from './deep-link.service'

function setup(links: string[]): MockInstance<DeepLinkService['open']> {
  const backend = new MockBackendTransport()
  backend.deepLinks = links
  TestBed.configureTestingModule({
    providers: [
      provideMockBackend(backend),
      provideAppRouter(),
      provideLocationMocks(),
      { provide: ActivityService, useValue: { collect: vi.fn(() => Promise.resolve()) } },
    ],
  })
  return vi.spyOn(TestBed.inject(DeepLinkService), 'open')
}

describe('DeepLinkInboxService', () => {
  it('opens the launch link and the links that arrive through DeepLinkService', async () => {
    const open = setup(['activity-log://timeline', 'activity-log://day/2026-11-17?q=standup'])

    await TestBed.inject(DeepLinkInboxService).start()
    // The first navigation, as the app makes at startup.
    await TestBed.inject(Router).navigateByUrl('/week/2026-11-16')

    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(2))
    expect(open.mock.calls).toEqual([
      ['activity-log://timeline'],
      ['activity-log://day/2026-11-17?q=standup'],
    ])
    await vi.waitFor(() => expect(TestBed.inject(Router).url).toBe('/day/2026-11-17?q=standup'))
  })

  it('waits for the first navigation, so the startup route does not replace the link', async () => {
    const open = setup(['activity-log://timeline'])

    await TestBed.inject(DeepLinkInboxService).start()
    await Promise.resolve()

    expect(open).not.toHaveBeenCalled()
    await TestBed.inject(Router).navigateByUrl('/week/2026-11-16')
    await vi.waitFor(() => expect(TestBed.inject(Router).url).toBe('/timeline'))
  })
})
