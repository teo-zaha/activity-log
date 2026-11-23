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
import { ConnectionSettings } from './connection-settings.service'

describe('ConnectionSettings', () => {
  it('shows the new sources and follows the collection the backend starts for them', async () => {
    vi.useFakeTimers()
    onTestFinished(() => void vi.useRealTimers())
    const backend = new MockBackendTransport()
    TestBed.configureTestingModule({
      providers: [provideMockBackend(backend)],
    })
    const data = TestBed.inject(ActivityDataService)
    await data.initLoad()
    backend.providersResponse = {
      currentOs: 'linux',
      secretStore: { store: 'os', state: 'ready' },
      providers: [...backend.providersResponse.providers, gitlabProvider],
    }
    backend.activityResponses = [collectingLog, activityLog]

    await TestBed.inject(ConnectionSettings).addConnection({ source: 'gitlab' })

    // The host pushes the new source and the collection it started for it.
    await vi.advanceTimersByTimeAsync(backend.stepMs)
    expect(data.providers().map(p => p.key)).toContain('gitlab/default')
    expect(data.isLoading()).toBe(true)
    expect(data.collectionStatus()).toMatchObject({ kind: 'collecting', operation: 'collect' })
    await vi.runAllTimersAsync()
    expect(data.isLoading()).toBe(false)
    expect(data.allEntries()).toEqual(activityLog.entries)
    expect(data.collectionStatus()).toEqual({ kind: 'idle' })
  })
})
