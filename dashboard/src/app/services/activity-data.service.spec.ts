import { TestBed } from '@angular/core/testing'
import { describe, expect, it, vi } from 'vitest'
import { activityLog, MockActivityBackendClient } from '../testing/activity-fixtures'
import {
  ActivityDataService,
  ACTIVITY_POLL_ATTEMPTS,
  ACTIVITY_POLL_DELAY_MS,
} from './activity-data.service'
import { ActivityViewStateService } from './activity-view-state.service'
import { ActivityBackendClient } from './backend/backend-client'

describe('ActivityDataService', () => {
  function setup(): { backend: MockActivityBackendClient; service: ActivityDataService } {
    const backend = new MockActivityBackendClient()
    TestBed.configureTestingModule({
      providers: [
        { provide: ActivityBackendClient, useValue: backend },
        { provide: ACTIVITY_POLL_ATTEMPTS, useValue: 2 },
        { provide: ACTIVITY_POLL_DELAY_MS, useValue: 1 },
      ],
    })
    return { backend, service: TestBed.inject(ActivityDataService) }
  }

  it('loads providers then activity during initial load', async () => {
    const { service } = setup()

    await service.initLoad()

    expect(service.providerInfo().length).toBe(2)
    expect(service.allEntries()).toEqual(activityLog.entries)
  })

  it('sets timeout error when collection never completes', async () => {
    vi.useFakeTimers()
    const { backend, service } = setup()
    backend.activityResponses = [{ collecting: true }, { collecting: true }]

    const promise = service.initLoad()
    await vi.runAllTimersAsync()
    await promise

    expect(service.error()).toBe('Timed out waiting for initial collection.')
    vi.useRealTimers()
  })

  it('collects immediate activity logs', async () => {
    const { service } = setup()

    await service.collect('2026-05-18', '2026-05-18')

    expect(service.allEntries()).toEqual(activityLog.entries)
  })

  it('updates date state from loaded metadata', async () => {
    const { service } = setup()
    const viewState = TestBed.inject(ActivityViewStateService)

    await service.initLoad()

    expect(viewState.dateFrom()).toBe('2026-05-17')
    expect(viewState.dateTo()).toBe('2026-05-18')
  })
})
