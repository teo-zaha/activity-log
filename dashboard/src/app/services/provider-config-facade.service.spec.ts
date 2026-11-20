import { TestBed } from '@angular/core/testing'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import {
  activityLog,
  collectingLog,
  gitlabProvider,
  MockBackendTransport,
  provideMockBackend,
} from '../testing/activity-fixtures'
import { ActivityDataService } from './activity-data.service'
import { ACTIVITY_POLL_DELAY_MS } from './runtime-state/polling-state-source'
import { ProviderConfigFacade } from './provider-config-facade.service'

describe('ProviderConfigFacade', () => {
  it('shows the new sources and follows the collection the backend starts for them', async () => {
    vi.useFakeTimers()
    onTestFinished(() => void vi.useRealTimers())
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

    // The answer has the host read at once: the new source, and the collection started for it.
    await vi.advanceTimersByTimeAsync(0)
    expect(data.providers().map(p => p.key)).toContain('gitlab/default')
    expect(data.isLoading()).toBe(true)
    expect(data.collectionStatus()).toMatchObject({ kind: 'collecting', operation: 'collect' })
    await vi.runAllTimersAsync()
    expect(data.isLoading()).toBe(false)
    expect(data.allEntries()).toEqual(activityLog.entries)
    expect(data.collectionStatus()).toEqual({ kind: 'idle' })
  })
})
