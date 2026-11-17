import { TestBed } from '@angular/core/testing'
import { describe, expect, it, vi } from 'vitest'
import {
  activityLog,
  collectingLog,
  gitlabProvider,
  MockBackendTransport,
  provideMockBackend,
} from '../testing/activity-fixtures'
import { ACTIVITY_POLL_DELAY_MS, ActivityDataService } from './activity-data.service'
import { ProviderConfigFacade } from './provider-config-facade.service'

describe('ProviderConfigFacade', () => {
  it('shows the new sources and follows the collection the backend starts for them', async () => {
    const backend = new MockBackendTransport()
    TestBed.configureTestingModule({
      providers: [provideMockBackend(backend), { provide: ACTIVITY_POLL_DELAY_MS, useValue: 1 }],
    })
    const data = TestBed.inject(ActivityDataService)
    await data.initLoad()
    backend.providersResponse = {
      currentOs: 'linux',
      providers: [...backend.providersResponse.providers, gitlabProvider],
    }
    backend.activityResponses = [collectingLog, activityLog]

    await TestBed.inject(ProviderConfigFacade).addConnection({ source: 'gitlab' })

    expect(data.providers().map(p => p.key)).toContain('gitlab/default')
    expect(data.isLoading()).toBe(true)
    await vi.waitFor(() => expect(data.isLoading()).toBe(false))
    expect(data.allEntries()).toEqual(activityLog.entries)
    expect(data.collectionStatus()).toEqual({ kind: 'idle' })
  })
})
