import { TestBed } from '@angular/core/testing'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { ActivityLog } from '../generated/activity'
import { BackendOperations } from './backend/backend-contract'
import {
  activityLog,
  collectingLog,
  gitlabProvider,
  MockBackendTransport,
  provideMockBackend,
} from '../testing/activity-fixtures'
import {
  ActivityDataService,
  ACTIVITY_POLL_ATTEMPTS,
  ACTIVITY_POLL_DELAY_MS,
} from './activity-data.service'
import { ActivityViewStateService } from './activity-view-state.service'

/**
 * A collection still running with nothing in yet, 4–6 May (noon to noon, so every time zone
 * sees the same days).
 */
const running: ActivityLog = {
  meta: {
    generated: '2026-05-06T12:00:00Z',
    start: '2026-05-04T12:00:00Z',
    end: '2026-05-06T12:00:00Z',
    pending: [{ source: 'git', name: 'default' }],
  },
  summary: { connections: [], mergedCount: 0 },
  entries: [],
}

describe('ActivityDataService', () => {
  function setup(pollAttempts = 2): {
    backend: MockBackendTransport
    service: ActivityDataService
    viewState: ActivityViewStateService
  } {
    const backend = new MockBackendTransport()
    TestBed.configureTestingModule({
      providers: [
        provideMockBackend(backend),
        { provide: ACTIVITY_POLL_ATTEMPTS, useValue: pollAttempts },
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

  it('knows when no source is set up, but only once providers loaded', () => {
    const { service } = setup()

    expect(service.hasNoSources()).toBe(false)
    service.applyProviders([], true)
    expect(service.hasNoSources()).toBe(true)
  })

  it('sets timeout error when collection never completes', async () => {
    vi.useFakeTimers()
    const { backend, service, viewState } = setup()
    backend.activityResponses = [running, running]

    const promise = service.initLoad()
    await vi.runAllTimersAsync()
    await promise

    expect(viewState.dateFrom()).toBe('2026-05-04')
    expect(viewState.dateTo()).toBe('2026-05-06')
    expect(service.error()).toBe('Timed out waiting for initial collection.')
    expect(service.collectionStatus()).toEqual({
      kind: 'timedOut',
      operation: 'initial',
      message: 'Timed out waiting for initial collection.',
    })
    vi.useRealTimers()
  })

  describe('while a collection runs', () => {
    beforeEach(() => vi.useFakeTimers())
    afterEach(() => vi.useRealTimers())

    /** Lets the service handle the answers it already has, up to its next wait. */
    const settleAnswers = (): Promise<unknown> => vi.advanceTimersByTimeAsync(0)

    function activityFetches(backend: MockBackendTransport): number {
      return vi
        .mocked(backend.call)
        .mock.calls.filter(([operation]) => operation === BackendOperations.activity).length
    }

    it('shows the connections that are in and marks the rest as collecting', async () => {
      const { backend, service } = setup()
      vi.spyOn(backend, 'call')
      backend.activityResponses = [collectingLog, activityLog]

      const load = service.initLoad()
      await settleAnswers()

      expect(service.allEntries()).toEqual(collectingLog.entries)
      expect([...service.pendingConnections()]).toEqual(['jira/default'])
      expect(service.providers().map(p => [p.key, p.collecting])).toEqual([
        ['git/default', false],
        ['jira/default', true],
      ])
      expect(service.isLoading()).toBe(true)

      await vi.runAllTimersAsync()
      await load

      expect(service.allEntries()).toEqual(activityLog.entries)
      expect(service.pendingConnections().size).toBe(0)
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
      expect(activityFetches(backend)).toBe(2)
    })

    it('does not show an answer again that brings nothing new', async () => {
      const { backend, service } = setup(3)
      backend.activityResponses = [
        collectingLog,
        { ...collectingLog, entries: [...collectingLog.entries] },
        activityLog,
      ]

      const load = service.initLoad()
      await settleAnswers()
      const shown = service.allEntries()
      await vi.advanceTimersByTimeAsync(1)

      expect(service.allEntries()).toBe(shown)

      await vi.runAllTimersAsync()
      await load
      expect(service.allEntries()).toEqual(activityLog.entries)
    })

    it('clears the selection when a collection starts, not when entries arrive', async () => {
      const { backend, service, viewState } = setup()
      await service.initLoad()
      viewState.selectedEntry.set(service.allEntryAdapters()[0] ?? null)
      backend.collectResponse = collectingLog
      backend.activityResponses = [activityLog]

      const collect = service.collect('2026-05-17', '2026-05-18')
      expect(viewState.selectedEntry()).toBeNull()
      await settleAnswers()
      const selected = service.allEntryAdapters()[0] ?? null
      viewState.selectedEntry.set(selected)

      await vi.runAllTimersAsync()
      await collect

      expect(selected).not.toBeNull()
      expect(viewState.selectedEntry()).toBe(selected)
    })

    it('follows the collection after a settings change without clearing the selection', async () => {
      const { backend, service, viewState } = setup()
      await service.initLoad()
      const selected = service.allEntryAdapters()[0] ?? null
      viewState.selectedEntry.set(selected)
      backend.activityResponses = [{ ...collectingLog, entries: [] }, activityLog]

      const follow = service.followCollection()
      await settleAnswers()

      expect(service.collectionStatus()).toMatchObject({ kind: 'collecting', operation: 'collect' })
      expect(service.allEntries().map(entry => entry.source)).toEqual(['jira'])
      expect(viewState.selectedEntry()).toBe(selected)

      await vi.runAllTimersAsync()
      await follow
      expect(service.allEntries()).toEqual(activityLog.entries)
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('keeps the log when the sources cannot be read once it is complete', async () => {
      const { backend, service } = setup()
      backend.activityResponses = [collectingLog, activityLog]

      const load = service.initLoad()
      await settleAnswers()
      backend.providersError = new Error('offline')
      await vi.runAllTimersAsync()
      await load

      expect(service.allEntries()).toEqual(activityLog.entries)
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
      expect(service.error()).toBeNull()
    })

    it('drops a sources answer that is older than the list a settings change brought', async () => {
      const { backend, service, viewState } = setup()
      const [git, jira] = backend.providersResponse.providers
      if (!git || !jira) throw new Error('fixture has two providers')
      const checking = { status: 'checking', usable: false } as const
      backend.providersResponse = { currentOs: 'macos', providers: [{ ...git, ...checking }] }
      await service.loadProviders()

      // The poll asks while only Git exists; its answer is held back.
      let answerPoll = (): void => undefined
      backend.providersResponse = { currentOs: 'macos', providers: [git] }
      backend.providersGate = new Promise(resolve => (answerPoll = resolve))
      await vi.advanceTimersByTimeAsync(1)
      backend.providersGate = null

      // Meanwhile a settings change adds Jira, still being checked.
      service.applyProviders([git, { ...jira, ...checking }], true)
      backend.providersResponse = { currentOs: 'macos', providers: [git, jira] }
      answerPoll()
      await settleAnswers()

      expect(service.providerInfo().map(info => info.status)).toEqual(['available', 'checking'])
      expect(viewState.activeConnections().has('jira/default')).toBe(true)

      // The poll goes on for the source that is still checking.
      await vi.advanceTimersByTimeAsync(1)
      expect(service.providerInfo().map(info => info.status)).toEqual(['available', 'available'])
    })

    it('keeps what was shown for a connection until it is collected again', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      backend.collectResponse = { ...collectingLog, entries: [] }
      backend.activityResponses = [activityLog]

      const collect = service.collect('2026-05-17', '2026-05-18')
      await settleAnswers()

      // Git is in with nothing; Jira is pending and keeps its entry from before.
      expect(service.allEntries().map(entry => entry.source)).toEqual(['jira'])

      await vi.runAllTimersAsync()
      await collect
      expect(service.allEntries()).toEqual(activityLog.entries)
    })
  })

  it('shows the range of a failed collection with its error', async () => {
    const { backend, service, viewState } = setup()
    backend.activityResponses = [
      {
        start: running.meta.start,
        end: running.meta.end,
        error: 'GitLab unreachable: connection refused',
      },
    ]

    await service.initLoad()

    expect(service.collectionStatus()).toEqual({
      kind: 'failed',
      operation: 'initial',
      message: 'GitLab unreachable: connection refused',
    })
    expect(viewState.dateFrom()).toBe('2026-05-04')
    expect(viewState.dateTo()).toBe('2026-05-06')
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

    expect(backend.requests).toContainEqual({
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
          connection: { source: 'outlook', name: 'default' },
          label: 'Outlook',
          status: 'unavailable',
          usable: false,
          reason: 'Startup check timed out after 15s',
          supportedOs: ['macos', 'windows'],
          checkedAt: '2026-05-18T08:00:00Z',
        },
      ],
    }

    await service.loadProviders()

    expect(service.providers().map(p => p.key)).toEqual(['git/default', 'jira/default'])
    expect(service.unusableProviders().map(p => p.info.reason)).toEqual([
      'Startup check timed out after 15s',
    ])
  })

  it('names a source by its own label, even when it is the only one of its kind', async () => {
    const { backend, service } = setup()
    const [git] = backend.providersResponse.providers
    if (!git) throw new Error('fixture has a provider')
    backend.providersResponse = { currentOs: 'macos', providers: [{ ...git, label: 'Laptop' }] }

    await service.loadProviders()

    expect(service.providers().map(p => p.label)).toEqual(['Laptop'])
  })

  it('lists sources being checked and asks again until their checks finish', async () => {
    const { backend, service, viewState } = setup()
    const [git] = backend.providersResponse.providers
    if (!git) throw new Error('fixture has a provider')
    backend.providersResponse = {
      currentOs: 'macos',
      providers: [{ ...git, status: 'checking', usable: false }],
    }

    await service.loadProviders()

    expect(service.providers().map(p => p.key)).toEqual(['git/default'])
    expect(service.unusableProviders()).toEqual([])
    // Shown from the start: its entries may arrive before the check is known to be done.
    expect([...viewState.activeConnections()]).toEqual(['git/default'])

    backend.providersResponse = { currentOs: 'macos', providers: [git] }
    await vi.waitFor(() => expect(service.providerInfo()[0]?.status).toBe('available'))
    expect([...viewState.activeConnections()]).toEqual(['git/default'])
  })

  it('applies each check as it ends and keeps the sources the user switched off', async () => {
    const { backend, service, viewState } = setup()
    const [git, jira] = backend.providersResponse.providers
    if (!git || !jira) throw new Error('fixture has two providers')
    const checking = { status: 'checking', usable: false } as const
    backend.providersResponse = {
      currentOs: 'macos',
      providers: [
        { ...git, ...checking },
        { ...jira, ...checking },
      ],
    }
    await service.loadProviders()
    viewState.activeConnections.set(new Set(['jira/default']))

    backend.providersResponse = { currentOs: 'macos', providers: [git, { ...jira, ...checking }] }
    await vi.waitFor(() => expect(service.providerInfo()[0]?.status).toBe('available'))
    expect(service.providerInfo()[1]?.status).toBe('checking')

    backend.providersResponse = {
      currentOs: 'macos',
      providers: [git, { ...jira, status: 'unavailable', usable: false }],
    }
    await vi.waitFor(() => expect(service.providerInfo()[1]?.status).toBe('unavailable'))
    expect(viewState.activeConnections().size).toBe(0)
  })

  it('shows a source the backend added by itself once a collection brings its entries', async () => {
    const { backend, service, viewState } = setup()
    await service.initLoad()
    viewState.activeConnections.set(new Set(['jira/default']))
    // `config.toml` changed under the running server: it now collects GitLab too.
    backend.providersResponse = {
      currentOs: 'linux',
      providers: [...backend.providersResponse.providers, gitlabProvider],
    }

    await service.collect('2026-05-17', '2026-05-18')

    expect(service.providers().map(p => p.key)).toContain('gitlab/default')
    expect([...viewState.activeConnections()].sort()).toEqual(['gitlab/default', 'jira/default'])
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
