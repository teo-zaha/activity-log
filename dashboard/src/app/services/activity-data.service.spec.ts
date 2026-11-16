import { TestBed } from '@angular/core/testing'
import { describe, expect, it, vi } from 'vitest'
import { activityLog, MockBackendTransport, provideMockBackend } from '../testing/activity-fixtures'
import {
  ActivityDataService,
  ACTIVITY_POLL_ATTEMPTS,
  ACTIVITY_POLL_DELAY_MS,
} from './activity-data.service'
import { ActivityViewStateService } from './activity-view-state.service'

describe('ActivityDataService', () => {
  function setup(): {
    backend: MockBackendTransport
    service: ActivityDataService
    viewState: ActivityViewStateService
  } {
    const backend = new MockBackendTransport()
    TestBed.configureTestingModule({
      providers: [
        provideMockBackend(backend),
        { provide: ACTIVITY_POLL_ATTEMPTS, useValue: 2 },
        { provide: ACTIVITY_POLL_DELAY_MS, useValue: 1 },
      ],
    })
    return {
      backend,
      service: TestBed.inject(ActivityDataService),
      viewState: TestBed.inject(ActivityViewStateService),
    }
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
    expect(service.collectionStatus()).toEqual({
      kind: 'timedOut',
      operation: 'initial',
      message: 'Timed out waiting for initial collection.',
    })
    vi.useRealTimers()
  })

  it('tracks collection progress and failures', async () => {
    const { backend, service } = setup()
    backend.collectError = new Error('boom')

    const promise = service.collect('2026-05-18', '2026-05-18')
    expect(service.collectionStatus().kind).toBe('collecting')
    await promise

    expect(service.collectionStatus()).toEqual({
      kind: 'failed',
      operation: 'collect',
      message: 'boom',
    })

    service.dismissCollectionStatus()
    expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    expect(service.error()).toBeNull()
  })

  it('collects whole local days as instants and shows the collected days', async () => {
    const { backend, service, viewState } = setup()
    backend.collectResponse = {
      ...activityLog,
      meta: {
        ...activityLog.meta,
        start: new Date(2026, 4, 17).toISOString(),
        end: new Date(2026, 4, 19).toISOString(),
      },
    }

    await service.collect('2026-05-17', '2026-05-18')

    expect(backend.requests.at(-1)).toEqual({
      start: new Date(2026, 4, 17).toISOString(),
      end: new Date(2026, 4, 19).toISOString(),
    })
    expect(viewState.dateFrom()).toBe('2026-05-17')
    expect(viewState.dateTo()).toBe('2026-05-18')
  })

  it('returns to idle after a successful collection', async () => {
    const { service } = setup()

    await service.collect('2026-05-18', '2026-05-18')

    expect(service.collectionStatus()).toEqual({ kind: 'idle' })
  })

  it('splits usable and unusable providers', async () => {
    const { backend, service } = setup()
    backend.providersResponse = {
      currentOs: 'macos',
      providers: [
        ...backend.providersResponse.providers,
        {
          id: 'outlook',
          status: 'unavailable',
          usable: false,
          reason: 'Startup check timed out after 15s',
          supportedOs: ['macos', 'windows'],
          checkedAt: '2026-05-18T08:00:00Z',
        },
      ],
    }

    await service.loadProviders()

    expect(service.providers().map(p => p.info.id)).toEqual(['git', 'jira'])
    expect(service.unusableProviders().map(p => p.info.reason)).toEqual([
      'Startup check timed out after 15s',
    ])
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
