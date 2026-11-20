import { provideLocationMocks } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { describe, expect, it, type MockInstance, vi } from 'vitest'
import { provideAppRouter } from '../app.routes'
import { MockBackendTransport, provideMockBackend } from '../testing/activity-fixtures'
import { ActivityService } from './activity.service'
import { DeepLinkInboxService } from './deep-link-inbox.service'
import { DeepLinkService } from './deep-link.service'
import { BackendFailure } from './backend/backend-failure'
import { BackendTransport } from './backend/backend-transport'

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

  it('starts the app without the listener when the shell refuses it, and logs why', async () => {
    setup([])
    const refused = new BackendFailure('internal', 'event.listen not allowed')
    vi.spyOn(TestBed.inject(BackendTransport), 'listenForDeepLinks').mockRejectedValue(refused)
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    await expect(TestBed.inject(DeepLinkInboxService).start()).resolves.toBeUndefined()

    expect(warn).toHaveBeenCalledWith(expect.stringContaining('event.listen not allowed'), refused)
  })
})
