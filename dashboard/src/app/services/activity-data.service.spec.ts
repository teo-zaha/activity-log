import { Location } from '@angular/common'
import { provideLocationMocks, SpyLocation } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type {
  ActivityLog,
  ActivityUpdate,
  PartStamp,
  ProviderConfigSaveResponseDto,
  ProviderInfo,
  ProvidersUpdate,
  RuntimeState,
} from '../generated/activity'
import { BackendOperations } from './backend/backend-contract'
import {
  activityLog,
  collectingLog,
  gitlabProvider,
  MockBackendTransport,
  providerInfo,
  provideMockBackend,
} from '../testing/activity-fixtures'
import { ActivityDataService } from './activity-data.service'
import { ActivityBackendClient } from './backend/backend-client'
import { type RuntimeStateSink, RuntimeStateSource } from './runtime-state/runtime-state-source'
import { FIRST_PAIR_TIMEOUT_MS } from './runtime-state/push-state-source'
import { ActivityViewStateService } from './activity-view-state.service'
import { ProviderConfigFacade } from './provider-config-facade.service'
import { localDayRange } from '../utils/local-day'
import { provideAppRouter } from '../app.routes'
import { BackendFailure } from './backend/backend-failure'

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

const jiraEntries = activityLog.entries.filter(entry => entry.source === 'jira')
const withEntries = (log: ActivityLog, entries: ActivityLog['entries']): ActivityLog => ({
  ...log,
  entries,
})

/** Lets the service handle the answers it already has, up to its next wait. */
const settleAnswers = (): Promise<unknown> => vi.advanceTimersByTimeAsync(0)

/** Lets the mock host take one step: it pushes what a command changed. */
const pushed = (backend: MockBackendTransport): Promise<unknown> =>
  vi.advanceTimersByTimeAsync(backend.stepMs)

/** Long enough for every reconnect to be tried and to fail. */
const GIVE_UP_MS = 60_000

function setup(stepMs = 1): {
  backend: MockBackendTransport
  service: ActivityDataService
  viewState: ActivityViewStateService
} {
  const backend = new MockBackendTransport()
  backend.stepMs = stepMs
  TestBed.configureTestingModule({ providers: [provideMockBackend(backend)] })
  return {
    backend,
    service: TestBed.inject(ActivityDataService),
    viewState: TestBed.inject(ActivityViewStateService),
  }
}

const save = (): Promise<ProviderConfigSaveResponseDto> =>
  TestBed.inject(ProviderConfigFacade).saveProviderConfig({
    connection: { source: 'jira', name: 'default' },
    label: 'Work',
    values: [],
  })

function withJira(status: ProviderInfo['status']): ProviderInfo[] {
  return providerInfo.map(provider =>
    provider.connection.source === 'jira' ? { ...provider, status, usable: false } : provider
  )
}

function calls(backend: MockBackendTransport, operation: unknown): unknown[] {
  return vi
    .mocked(backend.call)
    .mock.calls.filter(([called]) => called === operation)
    .map(([, request]) => request)
}

function held(): { gate: Promise<void>; release: () => void } {
  let release = (): void => undefined
  const gate = new Promise<void>(resolve => (release = resolve))
  return { gate, release }
}

describe('ActivityDataService', () => {
  beforeEach(() => vi.useFakeTimers())
  afterEach(() => vi.useRealTimers())

  it('loads the host’s sources and collection', async () => {
    const { service } = setup()

    await service.initLoad()

    expect(service.providerInfo().length).toBe(2)
    expect(service.allEntries()).toEqual(activityLog.entries)
  })

  it('knows when no source is set up, but only once providers loaded', async () => {
    const { backend, service } = setup()
    backend.providersResponse = { currentOs: 'linux', providers: [] }
    // Without sources there is no collection to wait for.
    backend.activityResponses = [null]
    backend.activityResponse = null

    expect(service.hasNoSources()).toBe(false)
    await service.initLoad()
    expect(service.hasNoSources()).toBe(true)
    expect(service.isLoading()).toBe(false)
    expect(service.collectionStatus()).toEqual({ kind: 'idle' })
  })

  it('waits for the first collection while the host has none yet', async () => {
    const { backend, service } = setup()
    backend.activityResponses = [null, null, collectingLog]

    const load = service.initLoad()
    await vi.advanceTimersByTimeAsync(1)
    expect(service.meta()).toBeNull()
    expect(service.collectionStatus()).toMatchObject({ kind: 'collecting', operation: 'initial' })

    await vi.runAllTimersAsync()
    await load
    expect(service.allEntries()).toEqual(activityLog.entries)
    expect(service.collectionStatus()).toEqual({ kind: 'idle' })
  })

  it('reports a backend it cannot reach, and Retry reconnects without collecting', async () => {
    const { backend, service } = setup()
    vi.spyOn(backend, 'call')
    const unreachable = new BackendFailure('unreachable', 'connection refused')
    backend.stateError = unreachable

    const load = service.initLoad()
    await vi.advanceTimersByTimeAsync(1)
    expect(service.collectionStatus()).toMatchObject({ kind: 'collecting' })
    expect(service.connection().status).toEqual({
      kind: 'reconnecting',
      failure: expect.objectContaining({ message: 'connection refused' }),
    })
    await vi.runAllTimersAsync()
    await load
    backend.stateError = null

    expect(service.collectionStatus()).toEqual({
      kind: 'failed',
      operation: 'initial',
      origin: 'lostBackend',
      failure: { kind: 'backend', message: 'connection refused', cause: unreachable },
    })

    await service.retryCollection()
    expect(service.allEntries()).toEqual(activityLog.entries)
    expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    expect(calls(backend, BackendOperations.collect)).toEqual([])
    expect(backend.streamsOpened).toBe(1 + 5 + 1)
  })

  it('follows a collection asked for after it gave up on the backend', async () => {
    const { backend, service } = setup()
    backend.stateError = new BackendFailure('unreachable', 'connection refused')
    const load = service.initLoad()
    await vi.runAllTimersAsync()
    await load
    expect(service.collectionStatus()).toMatchObject({ kind: 'failed' })

    // The backend is back: its answer to the collect has the dashboard connect again.
    backend.stateError = null
    backend.collectResponse = collectingLog
    backend.activityResponses = [collectingLog]
    await service.collect('2026-05-17', '2026-05-18')
    expect(service.collectionStatus()).toMatchObject({ kind: 'collecting', operation: 'collect' })
    await vi.runAllTimersAsync()
    expect(service.allEntries()).toEqual(activityLog.entries)
    expect(service.collectionStatus()).toEqual({ kind: 'idle' })
  })

  describe('opened on a URL with a range', () => {
    const range = localDayRange('2026-05-17', '2026-05-18')

    function openOnRange(): ReturnType<typeof setup> {
      TestBed.configureTestingModule({ providers: [provideLocationMocks()] })
      const parts = setup()
      TestBed.inject(Location).go('/day/2026-05-18?from=2026-05-17&to=2026-05-18')
      vi.spyOn(parts.backend, 'call')
      return parts
    }

    it('collects that range', async () => {
      const { backend, service, viewState } = openOnRange()

      await service.initLoad()

      expect(calls(backend, BackendOperations.collect)).toEqual([range])
      expect(service.allEntries()).toEqual(activityLog.entries)
      expect([viewState.dateFrom(), viewState.dateTo()]).toEqual(['2026-05-17', '2026-05-18'])
    })

    it('keeps the host a command answer showed when an older first pair brings another', async () => {
      const { backend, service } = openOnRange()
      const client = TestBed.inject(ActivityBackendClient)
      const firstPair = held()
      const post = held()
      // Host a sent the first pair before it was replaced by host b, which shows nothing.
      backend.epoch = 'a'
      backend.stateGate = firstPair.gate
      vi.spyOn(client, 'collect').mockImplementationOnce(() =>
        post.gate.then(() => ({
          ...withEntries(activityLog, []),
          kind: 'ready',
          stamp: { epoch: 'b', authority: '1', revision: 2 },
        }))
      )
      const starting = service.initLoad()
      await settleAnswers()
      backend.epoch = 'b'
      backend.stateGate = null
      backend.activityResponses = []
      backend.activityResponse = withEntries(activityLog, [])
      post.release()
      await settleAnswers()
      expect(service.hostEpoch()).toBe('b')

      firstPair.release()
      await settleAnswers()
      // Host a's pair conflicts with what host b answered: a fresh stream decides.
      expect(service.allEntries()).toEqual([])
      expect(backend.streamsOpened).toBe(2)

      await vi.runAllTimersAsync()
      await starting
      expect(service.allEntries()).toEqual([])
      expect(service.hostEpoch()).toBe('b')
    })

    it('follows the host without waiting for the collect to be answered', async () => {
      const { backend, service } = openOnRange()
      const post = held()
      backend.collectGate = post.gate

      const load = service.initLoad()
      await settleAnswers()

      expect(backend.streamsOpened).toBe(1)
      expect(service.providersLoaded()).toBe(true)
      post.release()
      await vi.runAllTimersAsync()
      await load
    })

    it('shows the accepted collect while the first pair is still out, and sends it once', async () => {
      const { backend, service } = openOnRange()
      const partial: ActivityLog = { ...collectingLog, meta: { ...collectingLog.meta, ...range } }
      backend.collectResponses = [partial]
      const read = held()
      backend.stateGate = read.gate

      const load = service.initLoad()
      const again = service.initLoad()
      await settleAnswers()

      expect(service.meta()).toEqual(partial.meta)
      backend.stateGate = null
      read.release()
      await vi.runAllTimersAsync()
      await Promise.all([load, again])
      expect(calls(backend, BackendOperations.collect)).toEqual([range])
    })
  })

  describe('while a collection runs', () => {
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
      expect(backend.streamsOpened).toBe(1)
    })

    it('changes nothing for a state the host repeats', async () => {
      const { backend, service } = setup()
      backend.activityResponses = [
        collectingLog,
        withEntries(collectingLog, [...collectingLog.entries]),
      ]

      const load = service.initLoad()
      await settleAnswers()
      const shown = service.allEntries()
      await vi.advanceTimersByTimeAsync(1)

      expect(service.allEntries()).toBe(shown)
      await vi.runAllTimersAsync()
      await load
    })

    it('follows the collection after a settings change', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      // The backend carries Jira's entry while Jira is collected again.
      backend.activityResponses = [withEntries(collectingLog, jiraEntries), activityLog]

      await save()
      await pushed(backend)

      expect(service.collectionStatus()).toMatchObject({ kind: 'collecting', operation: 'collect' })
      expect(service.allEntries().map(entry => entry.source)).toEqual(['jira'])

      await vi.runAllTimersAsync()
      expect(service.allEntries()).toEqual(activityLog.entries)
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('checks a source again and follows the collection the backend starts after it', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      backend.providersResponse = { currentOs: 'linux', providers: withJira('checking') }
      backend.activityResponses = [collectingLog]

      await service.recheckConnection({ source: 'jira', name: 'default' })
      expect(service.providerInfo()[1]?.status).toBe('checking')
      await pushed(backend)
      expect(service.collectionStatus()).toMatchObject({ kind: 'collecting', operation: 'collect' })

      backend.providersResponse = { currentOs: 'linux', providers: providerInfo }
      await vi.runAllTimersAsync()
      expect(service.providerInfo()[1]?.status).toBe('available')
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('connects again after a lost stream and follows the checks to their end', async () => {
      const { backend, service } = setup()
      backend.providersResponse = { currentOs: 'linux', providers: withJira('checking') }
      const load = service.initLoad()
      await settleAnswers()
      expect(service.providerInfo()[1]?.status).toBe('checking')

      backend.stateError = new BackendFailure('unreachable', 'connection reset')
      await vi.advanceTimersByTimeAsync(1)
      backend.stateError = null
      backend.providersResponse = { currentOs: 'linux', providers: providerInfo }
      await vi.runAllTimersAsync()
      await load

      expect(service.providerInfo().map(info => info.status)).toEqual(['available', 'available'])
    })

    it('keeps following after one lost stream among good ones', async () => {
      // The host drops the stream once: one reconnect, not the budget used up.
      const { backend, service } = setup()
      backend.activityResponses = [
        collectingLog,
        collectingLog,
        new BackendFailure('unreachable', 'connection reset'),
      ]

      const load = service.initLoad()
      await vi.runAllTimersAsync()
      await load

      expect(service.connection().status.kind).toBe('open')
      expect(service.allEntries()).toEqual(activityLog.entries)
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('gives up on a stream that never brings its pair', async () => {
      const { backend, service } = setup()
      backend.stateGate = new Promise(() => undefined)
      const load = service.initLoad()

      await vi.advanceTimersByTimeAsync(FIRST_PAIR_TIMEOUT_MS)
      expect(service.connection().status.kind).toBe('reconnecting')
      await vi.runAllTimersAsync()
      await load

      expect(service.collectionStatus()).toMatchObject({ kind: 'failed', operation: 'initial' })
    })

    it('clears the log when the host has none any more', async () => {
      const { backend, service, viewState } = setup()
      await service.initLoad()
      expect(service.meta()).not.toBeNull()
      backend.activityResponses = []
      backend.activityResponse = null
      backend.providersResponse = { currentOs: 'linux', providers: [] }

      await save()
      await vi.runAllTimersAsync()

      expect(service.allEntries()).toEqual([])
      expect(service.meta()).toBeNull()
      expect([viewState.dateFrom(), viewState.dateTo()]).toEqual([null, null])
    })

    it('shows a log whose entries were replaced under the same size', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      const replaced = jiraEntries.map(entry => ({ ...entry, id: 'AL-9:updated', key: 'AL-9' }))
      backend.activityResponses = [
        withEntries(collectingLog, jiraEntries),
        withEntries(collectingLog, replaced),
      ]

      await save()
      await pushed(backend)
      expect(service.allEntries()).toEqual(jiraEntries)
      await vi.advanceTimersByTimeAsync(1)

      expect(service.allEntries()).toEqual(replaced)
    })
  })

  describe('an account that changes (B69)', () => {
    it('shows the entries a check takes away while the same connection stays pending', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      backend.activityResponses = [
        withEntries(collectingLog, jiraEntries),
        withEntries(collectingLog, []),
      ]
      backend.activityResponse = withEntries(activityLog, [])

      await save()
      await pushed(backend)
      expect(service.allEntries()).toEqual(jiraEntries)

      await vi.advanceTimersByTimeAsync(1)
      expect(service.allEntries()).toEqual([])

      await vi.runAllTimersAsync()
      expect(service.allEntries()).toEqual([])
    })

    it('follows the collection a check starts after a label save', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      backend.providersResponse = { currentOs: 'linux', providers: withJira('checking') }
      // The host still shows the last log while the new check runs.
      backend.activityResponses = [activityLog]

      await save()
      await pushed(backend)
      expect(service.allEntries()).toEqual(activityLog.entries)

      // The check found another account: the host collected Jira again, with nothing for it.
      backend.providersResponse = { currentOs: 'linux', providers: providerInfo }
      backend.activityResponse = withEntries(activityLog, [])
      await vi.runAllTimersAsync()

      expect(service.providerInfo()).toEqual(providerInfo)
      expect(service.allEntries()).toEqual([])
    })
  })

  describe('commands', () => {
    it('stays busy until the latest of two collections has been answered', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      const first = held()
      const second = held()
      backend.collectGate = first.gate
      const firstCollect = service.collect('2026-05-17', '2026-05-18')
      // A deep link asks for another range while the first request is out.
      backend.collectGate = second.gate
      const secondCollect = service.collect('2026-05-10', '2026-05-18')
      backend.collectGate = null

      first.release()
      await firstCollect
      expect(service.isLoading()).toBe(true)

      second.release()
      await secondCollect
      await vi.runAllTimersAsync()
      expect(service.isLoading()).toBe(false)
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('gives a superseded collect no say in loading or failure', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      let refuse = (_: Error): void => undefined
      backend.collectGate = new Promise((_, reject) => (refuse = reject))
      const first = service.collect('2026-05-10', '2026-05-11')
      backend.collectGate = null
      await service.collect('2026-05-12', '2026-05-13')
      await vi.runAllTimersAsync()
      expect(service.isLoading()).toBe(false)

      refuse(new BackendFailure('internal', 'refused old range'))
      await first
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('ends a collect’s say once a check is sent after it', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      let refuse = (_: Error): void => undefined
      backend.collectGate = new Promise((_, reject) => (refuse = reject))
      const collect = service.collect('2026-05-10', '2026-05-11')
      backend.collectGate = null

      await service.recheckConnection({ source: 'jira', name: 'default' })
      await vi.runAllTimersAsync()
      expect(service.isLoading()).toBe(false)

      refuse(new BackendFailure('internal', 'refused old range'))
      await collect
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('never shows the late answer of a superseded collect', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      const first = held()
      backend.collectGate = first.gate
      backend.collectResponse = withEntries(collectingLog, jiraEntries)
      const firstCollect = service.collect('2026-05-10', '2026-05-11')
      backend.collectGate = null
      // The second collection finds nothing for Jira any more (another account).
      const empty = withEntries(activityLog, [])
      backend.activityResponses = []
      backend.activityResponse = empty
      backend.collectResponse = empty
      await service.collect('2026-05-12', '2026-05-13')
      await vi.runAllTimersAsync()
      expect(service.allEntries()).toEqual([])

      first.release()
      await firstCollect
      await vi.runAllTimersAsync()
      expect(service.allEntries()).toEqual([])
    })

    it('never brings back with a collect’s answer what a newer read took away', async () => {
      const { backend, service } = setup()
      // A collection runs, so the host is being read.
      backend.activityResponses = []
      backend.activityResponse = collectingLog
      void service.initLoad()
      await settleAnswers()
      const answer = held()
      backend.collectGate = answer.gate
      backend.collectResponse = withEntries(collectingLog, jiraEntries)
      const collect = service.collect('2026-05-10', '2026-05-11')
      backend.collectGate = null
      // While the answer travels, the host evicts Jira's entries and a read shows that.
      backend.activityResponse = withEntries(activityLog, [])
      await vi.runAllTimersAsync()
      expect(service.allEntries()).toEqual([])

      answer.release()
      await collect
      expect(service.allEntries()).toEqual([])
    })

    it('shows what the host pushes after a refused command and a superseded settings answer', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      const reply = held()
      backend.saveGate = reply.gate
      // The removal of Jira is installed; its answer is held.
      backend.providersResponse = { currentOs: 'linux', providers: [providerInfo[0]!] }
      const removal = TestBed.inject(ProviderConfigFacade).removeConnection({
        source: 'jira',
        name: 'default',
      })
      backend.saveGate = null
      // A check of Jira, sent after it, is refused: Jira is gone, which the read it brings shows.
      backend.recheckError = new BackendFailure('internal', 'No source jira/default')
      await expect(service.recheckConnection({ source: 'jira', name: 'default' })).rejects.toThrow()
      await pushed(backend)
      expect(service.providerInfo().map(info => info.connection.source)).toEqual(['git'])

      reply.release()
      await removal
      await vi.runAllTimersAsync()
      expect(service.providerInfo().map(info => info.connection.source)).toEqual(['git'])
    })

    it('shows a lost backend while a collect still waits for its answer', async () => {
      const { backend, service } = setup()
      // A collection runs, so the host pushes.
      backend.activityResponses = []
      backend.activityResponse = collectingLog
      void service.initLoad()
      await settleAnswers()
      backend.collectGate = new Promise(() => undefined)
      void service.collect('2026-05-10', '2026-05-11')
      backend.stateError = new BackendFailure('unreachable', 'connection refused')
      // The reconnects give up long before the collect's answer would count as lost.
      await vi.advanceTimersByTimeAsync(20_000)

      expect(service.collectionStatus()).toMatchObject({
        kind: 'failed',
        failure: { kind: 'backend', message: 'connection refused' },
      })
    })

    it('resolves a collect once it is accepted, while the collection runs on', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      backend.collectResponse = collectingLog
      backend.activityResponses = []
      backend.activityResponse = collectingLog

      await service.collect('2026-05-17', '2026-05-18')

      expect(service.isLoading()).toBe(true)
      backend.activityResponse = activityLog
      await vi.runAllTimersAsync()
      expect(service.isLoading()).toBe(false)
    })

    it('tracks a refused collect, until it is dismissed', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      backend.collectError = new BackendFailure('internal', 'boom')

      await service.collect('2026-05-18', '2026-05-18')

      expect(service.collectionStatus()).toEqual({
        kind: 'failed',
        operation: 'collect',
        origin: 'collect',
        failure: { kind: 'backend', message: 'boom', cause: backend.collectError },
      })
      service.dismissCollectionStatus()
      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('stops showing a refused collect once a later check is accepted', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      backend.collectError = new BackendFailure('internal', 'bad range')
      await service.collect('2026-05-20', '2026-05-10')
      expect(service.collectionStatus().kind).toBe('failed')

      await service.recheckConnection({ source: 'jira', name: 'default' })
      await vi.runAllTimersAsync()

      expect(service.collectionStatus().kind).toBe('idle')
    })

    it('collects whole local days as instants and shows the collected days', async () => {
      const { backend, service, viewState } = setup()
      await service.initLoad()
      const week: ActivityLog = {
        ...activityLog,
        meta: {
          ...activityLog.meta,
          start: new Date(2026, 4, 17).toISOString(),
          end: new Date(2026, 4, 25).toISOString(),
        },
      }
      backend.collectResponse = week
      backend.activityResponse = week

      await service.collect('2026-05-17', '2026-05-24')

      expect(backend.requests).toContainEqual({
        start: new Date(2026, 4, 17).toISOString(),
        end: new Date(2026, 4, 25).toISOString(),
      })
      expect(viewState.dateFrom()).toBe('2026-05-17')
      expect(viewState.dateTo()).toBe('2026-05-24')
    })

    it('shows a source the backend added by itself once a collection brings its entries', async () => {
      const { backend, service, viewState } = setup()
      await service.initLoad()
      viewState.activeConnections.set(new Set(['jira/default']))
      // `config.toml` changed under the running server: it now collects GitLab too.
      backend.providersResponse = {
        currentOs: 'linux',
        providers: [...providerInfo, gitlabProvider],
      }

      await service.collect('2026-05-17', '2026-05-18')
      await vi.runAllTimersAsync()

      expect(service.providers().map(p => p.key)).toContain('gitlab/default')
      expect([...viewState.activeConnections()].sort()).toEqual(['gitlab/default', 'jira/default'])
    })
  })

  it('offers Retry for a collect sent from an idle screen whose answer never comes', async () => {
    const { backend, service } = setup()
    await service.initLoad()
    backend.collectGate = new Promise(() => undefined)
    void service.collect('2026-05-10', '2026-05-11')
    backend.stateError = new BackendFailure('unreachable', 'connection refused')

    await vi.advanceTimersByTimeAsync(60 * 60 * 1000)

    expect(service.isLoading()).toBe(false)
    expect(service.collectionStatus().kind).toBe('failed')
  })

  it('shows what a command whose answer was lost did, without sending it again', async () => {
    const { backend, service } = setup()
    await service.initLoad()
    expect(service.allEntries()).toEqual(activityLog.entries)
    const recheck = vi
      .spyOn(TestBed.inject(ActivityBackendClient), 'recheckConnection')
      .mockImplementationOnce(() => {
        // The host checks again and evicts the entries, but its answer never arrives.
        backend.activityResponses = []
        backend.activityResponse = withEntries(activityLog, [])
        backend.publish()
        return Promise.reject(new BackendFailure('unreachable', 'connection reset'))
      })

    await expect(
      service.recheckConnection({ source: 'jira', name: 'default' })
    ).rejects.toMatchObject({ kind: 'unreachable' })
    await vi.advanceTimersByTimeAsync(60 * 60 * 1000)

    expect(service.allEntries()).toEqual([])
    expect(recheck).toHaveBeenCalledTimes(1)
  })

  describe('an answer that comes after its deadline', () => {
    it('shows what a recheck answered late did, but not its answer', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      const client = TestBed.inject(ActivityBackendClient)
      const late = held()
      const send = vi
        .spyOn(client, 'recheckConnection')
        .mockImplementationOnce(async connection => {
          await late.gate
          // The host applies the check only now, after the read the timeout brought.
          backend.activityResponse = withEntries(activityLog, [])
          return backend.call(BackendOperations.recheckConnection, connection)
        })
      const refused = expect(
        service.recheckConnection({ source: 'jira', name: 'default' })
      ).rejects.toMatchObject({ kind: 'unreachable' })
      await vi.advanceTimersByTimeAsync(31_000)
      await refused
      expect(service.allEntries()).toHaveLength(2)

      late.release()
      await vi.advanceTimersByTimeAsync(60 * 60 * 1000)

      expect(send).toHaveBeenCalledTimes(1)
      expect(service.allEntries()).toEqual([])
    })

    it('shows what a collect answered late did, and its range', async () => {
      const { backend, service, viewState } = setup()
      await service.initLoad()
      const client = TestBed.inject(ActivityBackendClient)
      const late = held()
      const send = vi.spyOn(client, 'collect').mockImplementationOnce(async range => {
        await late.gate
        backend.activityResponse = {
          ...activityLog,
          meta: { ...activityLog.meta, ...range },
          entries: [],
        }
        backend.collectResponse = backend.activityResponse
        return backend.call(BackendOperations.collect, range)
      })
      const collect = service.collect('2026-05-10', '2026-05-11')
      await vi.advanceTimersByTimeAsync(31_000)
      await collect
      expect(service.collectionStatus().kind).toBe('failed')

      late.release()
      await vi.advanceTimersByTimeAsync(60 * 60 * 1000)

      expect(send).toHaveBeenCalledTimes(1)
      expect(service.allEntries()).toEqual([])
      expect([viewState.dateFrom(), viewState.dateTo()]).toEqual(['2026-05-10', '2026-05-11'])
    })

    it('follows a collection acknowledged in time for as long as it runs', async () => {
      const { backend, service } = setup(1000)
      await service.initLoad()
      const send = vi.spyOn(TestBed.inject(ActivityBackendClient), 'collect')
      backend.collectResponse = running
      backend.activityResponse = running
      await service.collect('2026-05-04', '2026-05-06')
      await vi.advanceTimersByTimeAsync(65_000)
      expect(service.collectionStatus().kind).toBe('collecting')

      backend.activityResponse = activityLog
      await vi.advanceTimersByTimeAsync(1_000)

      expect(service.collectionStatus().kind).toBe('idle')
      expect(send).toHaveBeenCalledTimes(1)
    })

    it('keeps its stream for a recheck refused on a connected host', async () => {
      const { backend, service } = setup()
      await service.initLoad()
      const client = TestBed.inject(ActivityBackendClient)
      const send = vi.spyOn(client, 'recheckConnection')
      backend.recheckError = new BackendFailure('internal', 'lost')

      await expect(service.recheckConnection({ source: 'jira', name: 'default' })).rejects.toBe(
        backend.recheckError
      )
      await vi.advanceTimersByTimeAsync(60 * 60 * 1000)

      expect(backend.streamsOpened).toBe(1)
      expect(send).toHaveBeenCalledTimes(1)
    })
  })

  it('moves to a backend that restarted', async () => {
    const { backend, service } = setup()
    await service.initLoad()
    expect(service.allEntries()).toEqual(activityLog.entries)

    // The collect is answered by another host; the old one's stream ends.
    backend.epoch = 'restarted-host'
    backend.activityResponse = withEntries(activityLog, [])
    await service.collect('2026-05-10', '2026-05-11')
    await vi.runAllTimersAsync()

    expect(service.hostEpoch()).toBe('restarted-host')
    expect(service.allEntries()).toEqual([])
  })

  describe('teardown and retry', () => {
    it('takes nothing more and opens no stream once it is torn down', async () => {
      const { backend, service } = setup()
      backend.activityResponses = [collectingLog]
      void service.initLoad()
      await settleAnswers()
      const shown = service.allEntries()

      TestBed.resetTestingModule()
      await vi.runAllTimersAsync()

      expect(service.allEntries()).toBe(shown)
      expect(backend.streamsOpened).toBe(1)
    })

    it('delivers nothing once it is torn down while the first pair is on its way', async () => {
      const { backend, service } = setup()
      const pair = held()
      backend.stateGate = pair.gate
      void service.initLoad()
      await settleAnswers()

      TestBed.resetTestingModule()
      pair.release()
      await vi.runAllTimersAsync()

      expect(service.providersLoaded()).toBe(false)
      expect(backend.streamsOpened).toBe(1)
    })

    it('drops the pair of the stream Retry replaced', async () => {
      const { backend, service } = setup()
      backend.stateError = new BackendFailure('unreachable', 'connection refused')
      const load = service.initLoad()
      await vi.advanceTimersByTimeAsync(GIVE_UP_MS)
      await load
      expect(service.collectionStatus()).toMatchObject({ kind: 'failed' })
      backend.stateError = null
      // A pair from the host before a restart is held; Retry reconnects to the new one.
      const stale = held()
      backend.stateGate = stale.gate
      const retry = service.retryCollection()
      await settleAnswers()
      backend.stateGate = null
      backend.epoch = 'restarted-host'
      backend.activityResponse = withEntries(activityLog, [])
      await service.retryCollection()
      stale.release()
      await vi.runAllTimersAsync()
      await retry

      expect(service.allEntries()).toEqual([])
      expect(service.hostEpoch()).toBe('restarted-host')
    })
  })

  it('shows the range of a failed collection with its error', async () => {
    const { backend, service, viewState } = setup()
    const failed = {
      start: running.meta.start,
      end: running.meta.end,
      error: 'GitLab unreachable: connection refused',
    }
    backend.activityResponses = [failed]

    await service.initLoad()

    expect(service.collectionStatus()).toEqual({
      kind: 'failed',
      operation: 'initial',
      origin: 'collection',
      failure: { kind: 'other', message: 'GitLab unreachable: connection refused' },
    })
    expect(viewState.dateFrom()).toBe('2026-05-04')
    expect(viewState.dateTo()).toBe('2026-05-06')
  })

  it('shows a failed collection again once a lost backend is back', async () => {
    const { backend, service } = setup()
    const failed = { start: running.meta.start, end: running.meta.end, error: 'invalid report' }
    backend.activityResponses = []
    backend.activityResponse = failed
    await service.initLoad()
    const failedWith = (message: string): object => ({ kind: 'failed', failure: { message } })
    expect(service.collectionStatus()).toMatchObject(failedWith('invalid report'))

    backend.stateError = new BackendFailure('unreachable', 'offline')
    await save()
    await vi.runAllTimersAsync()
    expect(service.collectionStatus()).toMatchObject(failedWith('offline'))

    backend.stateError = null
    await service.retryCollection()
    expect(service.collectionStatus()).toMatchObject(failedWith('invalid report'))
  })

  it('dismisses one failure at a time, whatever their wording', async () => {
    const { backend, service } = setup()
    const failed = { start: running.meta.start, end: running.meta.end, error: 'boom' }
    backend.activityResponses = []
    backend.activityResponse = failed
    await service.initLoad()
    backend.collectError = new BackendFailure('internal', 'boom')
    await service.collect('2026-05-04', '2026-05-05')

    service.dismissCollectionStatus()
    // The refused request is dismissed; the failed collection, worded the same, still shows.
    expect(service.collectionStatus()).toMatchObject({
      kind: 'failed',
      failure: { kind: 'other', message: 'boom' },
    })
    service.dismissCollectionStatus()
    expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    await vi.advanceTimersByTimeAsync(5)
    expect(service.collectionStatus()).toEqual({ kind: 'idle' })
  })

  it('splits usable and unusable providers', async () => {
    const { backend, service } = setup()
    backend.providersResponse = {
      currentOs: 'macos',
      providers: [
        ...providerInfo,
        {
          connection: { source: 'outlook', name: 'default' },
          label: 'Outlook',
          status: 'unavailable',
          usable: false,
          recheckable: true,
          reason: 'Startup check timed out after 15s',
          supportedOs: ['macos', 'windows'],
          checkedAt: '2026-05-18T08:00:00Z',
        },
      ],
    }

    await service.initLoad()

    expect(service.providers().map(p => p.key)).toEqual(['git/default', 'jira/default'])
    expect(service.unusableProviders().map(p => p.info.reason)).toEqual([
      'Startup check timed out after 15s',
    ])
  })

  it('names a source by its own label, even when it is the only one of its kind', async () => {
    const { backend, service } = setup()
    backend.providersResponse = {
      currentOs: 'macos',
      providers: [{ ...providerInfo[0]!, label: 'Laptop' }],
    }

    await service.initLoad()

    expect(service.providers().map(p => p.label)).toEqual(['Laptop'])
  })

  it('applies each check as it ends and keeps the sources the user switched off', async () => {
    const { backend, service, viewState } = setup()
    const [git, jira] = providerInfo
    if (!git || !jira) throw new Error('fixture has two providers')
    const checking = { status: 'checking', usable: false } as const
    backend.providersResponse = {
      currentOs: 'macos',
      providers: [
        { ...git, ...checking },
        { ...jira, ...checking },
      ],
    }
    void service.initLoad()
    await settleAnswers()
    // Shown from the start: their entries may arrive before the checks are known to be done.
    expect([...viewState.activeConnections()].sort()).toEqual(['git/default', 'jira/default'])
    viewState.activeConnections.set(new Set(['jira/default']))

    backend.providersResponse = { currentOs: 'macos', providers: [git, { ...jira, ...checking }] }
    await vi.advanceTimersByTimeAsync(1)
    expect(service.providerInfo().map(info => info.status)).toEqual(['available', 'checking'])

    backend.providersResponse = {
      currentOs: 'macos',
      providers: [git, { ...jira, status: 'unavailable', usable: false }],
    }
    await vi.advanceTimersByTimeAsync(1)
    expect(service.providerInfo()[1]?.status).toBe('unavailable')
    expect(viewState.activeConnections().size).toBe(0)
  })

  describe('stamped state from a source that is told it (shared contracts §5)', () => {
    /** A source the test feeds by hand, as a pushing one would deliver. */
    class FedSource implements RuntimeStateSource {
      sink: RuntimeStateSink | null = null
      retries = 0
      follows = 0
      connect(sink: RuntimeStateSink): void {
        this.sink = sink
      }
      follow(): void {
        this.follows++
      }
      retry(): void {
        this.retries++
      }
      settled(): Promise<void> {
        return Promise.resolve()
      }
      feed(): RuntimeStateSink {
        if (this.sink === null) throw new Error('not connected')
        return this.sink
      }
    }

    const [git, jira] = providerInfo
    const stamp = (epoch: string, revision: number, authority = '1'): PartStamp => ({
      epoch,
      authority,
      revision,
    })
    const activity = (epoch: string, revision: number, authority = '1'): ActivityUpdate => ({
      ...stamp(epoch, revision, authority),
      state: { kind: 'ready', log: activityLog },
    })
    const providers = (
      epoch: string,
      revision: number,
      list = providerInfo,
      authority = '1'
    ): ProvidersUpdate => ({
      ...stamp(epoch, revision, authority),
      providers: { currentOs: 'linux', providers: list },
    })
    const pair = (
      epoch: string,
      revision: number,
      list = providerInfo,
      authority = '1'
    ): RuntimeState => ({
      activity: activity(epoch, revision, authority),
      providers: providers(epoch, revision, list, authority),
    })
    const saved = (epoch: string, revision: number): ProviderConfigSaveResponseDto => ({
      snapshot: { revision: 'r1', kinds: [], connections: [] },
      providersResponse: { currentOs: 'linux', providers: providerInfo },
      installed: { epoch, revision },
    })

    function connect(): {
      source: FedSource
      backend: MockBackendTransport
      service: ActivityDataService
      viewState: ActivityViewStateService
    } {
      const source = new FedSource()
      const backend = new MockBackendTransport()
      backend.epoch = 'a'
      TestBed.configureTestingModule({
        providers: [provideMockBackend(backend), { provide: RuntimeStateSource, useValue: source }],
      })
      const service = TestBed.inject(ActivityDataService)
      void service.initLoad()
      return { source, backend, service, viewState: TestBed.inject(ActivityViewStateService) }
    }

    describe('the range the URL holds', () => {
      const april: ActivityLog = {
        ...activityLog,
        meta: {
          ...activityLog.meta,
          start: new Date(2026, 3, 1).toISOString(),
          end: new Date(2026, 3, 9).toISOString(),
        },
      }
      /** Ready with `log`; `activityLog` covers 17–18 May. */
      const shown = (epoch: string, revision: number, log: ActivityLog): RuntimeState => ({
        activity: { ...stamp(epoch, revision), state: { kind: 'ready', log } },
        providers: providers(epoch, revision),
      })
      const MAY = '/timeline?from=2026-05-17&to=2026-05-18'
      const APRIL = '/timeline?from=2026-04-01&to=2026-04-08'

      async function connectAt(
        url: string
      ): Promise<ReturnType<typeof connect> & { router: Router; location: SpyLocation }> {
        TestBed.configureTestingModule({ providers: [provideAppRouter(), provideLocationMocks()] })
        const parts = connect()
        const router = TestBed.inject(Router)
        router.initialNavigation()
        await router.navigateByUrl(url)
        const location = TestBed.inject(Location)
        if (!(location instanceof SpyLocation)) throw new Error('location mocks are provided')
        location.urlChanges = []
        return { ...parts, router, location }
      }

      it('keeps a range picked when another client collects another', async () => {
        const { source, viewState, router } = await connectAt(MAY)
        source.feed().state(shown('a', 1, activityLog), true)
        await viewState.setDateFrom('2026-05-01')

        source.feed().activity({ ...stamp('a', 2), state: { kind: 'ready', log: april } })
        await vi.runAllTimersAsync()

        expect(router.url).toBe('/timeline?from=2026-05-01&to=2026-05-18')
        expect([viewState.dateFrom(), viewState.dateTo()]).toEqual(['2026-05-01', '2026-05-18'])
        // What the host shows stays on screen beside the range, as data of its own.
        expect(viewState.collectedRange()).toEqual({ from: '2026-04-01', to: '2026-04-08' })
      })

      it('keeps the range even where it follows the collection, for another client’s', async () => {
        const { source, router } = await connectAt(MAY)
        source.feed().state(shown('a', 1, activityLog), true)

        source.feed().activity({ ...stamp('a', 2), state: { kind: 'ready', log: april } })
        await vi.runAllTimersAsync()

        expect(router.url).toBe(MAY)
      })

      it('takes a restarted host’s range in place of the one it followed', async () => {
        const { source, viewState, router, location } = await connectAt(MAY)
        source.feed().state(shown('a', 1, activityLog), true)

        source.feed().state(shown('b', 1, april), true)
        await vi.runAllTimersAsync()

        expect(router.url).toBe(APRIL)
        expect([viewState.dateFrom(), viewState.dateTo()]).toEqual(['2026-04-01', '2026-04-08'])
        expect(location.urlChanges).toEqual([`replace: ${APRIL}`])
      })

      it('takes it only once the restarted host shows a range', async () => {
        const { source, router } = await connectAt(MAY)
        source.feed().state(shown('a', 1, activityLog), true)

        source.feed().state(
          {
            activity: { ...stamp('b', 1), state: { kind: 'empty' } },
            providers: providers('b', 1),
          },
          true
        )
        await vi.runAllTimersAsync()
        expect(router.url).toBe(MAY)

        source.feed().activity({ ...stamp('b', 2), state: { kind: 'running', log: april } })
        await vi.runAllTimersAsync()
        expect(router.url).toBe(APRIL)
      })

      it('waits while the range is edited, and keeps one picked meanwhile', async () => {
        const { source, viewState, router } = await connectAt(MAY)
        source.feed().state(shown('a', 1, activityLog), true)
        viewState.setRangeEditing(true)

        source.feed().state(shown('b', 1, april), true)
        await vi.runAllTimersAsync()
        expect(router.url).toBe(MAY)

        await viewState.setDateFrom('2026-05-01')
        viewState.setRangeEditing(false)
        await vi.runAllTimersAsync()
        expect(router.url).toBe('/timeline?from=2026-05-01&to=2026-05-18')
      })

      it('takes it once an edit ends that picked nothing', async () => {
        const { source, viewState, router } = await connectAt(MAY)
        source.feed().state(shown('a', 1, activityLog), true)
        viewState.setRangeEditing(true)

        source.feed().state(shown('b', 1, april), true)
        await vi.runAllTimersAsync()
        viewState.setRangeEditing(false)
        await vi.runAllTimersAsync()

        expect(router.url).toBe(APRIL)
      })

      it('lets a navigation under way finish, then decides for where it went', async () => {
        const { source, router } = await connectAt(MAY)
        source.feed().state(shown('a', 1, activityLog), true)

        // The day view loads its code on demand: the navigation is still on its way.
        const day = router.navigateByUrl('/day/2026-05-18?from=2026-05-17&to=2026-05-18')
        source.feed().state(shown('b', 1, april), true)
        // What reacts to the new host runs now, while the day view's code is still loading.
        TestBed.tick()
        expect(router.currentNavigation()).not.toBeNull()

        expect(await day).toBe(true)
        await vi.runAllTimersAsync()
        expect(router.url).toBe('/day/2026-05-18?from=2026-04-01&to=2026-04-08')
      })

      it('leaves the range to a collect on its way as the host restarts', async () => {
        const { source, backend, service, router } = await connectAt(MAY)
        source.feed().state(shown('a', 1, activityLog), true)
        backend.collectGate = held().gate

        void service.collect('2026-05-17', '2026-05-18')
        source.feed().state(shown('b', 1, april), true)
        await vi.advanceTimersByTimeAsync(1_000)

        expect(router.url).toBe(MAY)
      })
    })

    describe('the time of the run on screen', () => {
      const running = (epoch: string, revision: number, log = collectingLog): ActivityUpdate => ({
        ...stamp(epoch, revision),
        state: { kind: 'running', log },
      })
      const startedAt = (service: ActivityDataService): number | null => {
        const status = service.collectionStatus()
        return status.kind === 'collecting' ? status.startedAt : null
      }

      it('keeps it across the updates of one run', () => {
        const { source, service } = connect()
        source.feed().state({ activity: running('a', 1), providers: providers('a', 1) }, true)
        const started = Date.now()

        vi.advanceTimersByTime(5_000)
        source.feed().activity(running('a', 2))

        expect(startedAt(service)).toBe(started)
      })

      it('starts it again for a run that follows a finished one at once', () => {
        const { source, service } = connect()
        source.feed().state({ activity: running('a', 1), providers: providers('a', 1) }, true)
        expect(startedAt(service)).not.toBeNull()
        vi.advanceTimersByTime(200_000)

        // The run ends and the next begins, with nothing reading the status between them.
        source.feed().activity(activity('a', 2))
        source.feed().activity(running('a', 3))

        expect(startedAt(service)).toBe(Date.now())
      })

      it('starts it again for another host’s run, already running as it comes', () => {
        const { source, service } = connect()
        source.feed().state({ activity: running('a', 1), providers: providers('a', 1) }, true)
        expect(startedAt(service)).not.toBeNull()
        vi.advanceTimersByTime(30_000)

        source.feed().state({ activity: running('b', 1), providers: providers('b', 1) }, true)

        expect(service.hostEpoch()).toBe('b')
        expect(startedAt(service)).toBe(Date.now())
      })

      it('starts it again for a run of another range', () => {
        const { source, service } = connect()
        source.feed().state({ activity: running('a', 1), providers: providers('a', 1) }, true)
        expect(startedAt(service)).not.toBeNull()
        vi.advanceTimersByTime(30_000)

        const april = { ...collectingLog.meta, start: new Date(2026, 3, 1).toISOString() }
        source.feed().activity(running('a', 2, { ...collectingLog, meta: april }))

        expect(startedAt(service)).toBe(Date.now())
      })
    })

    it('installs another host whole and shows every usable source again', () => {
      const { source, service, viewState } = connect()
      source.feed().state(pair('a', 3), true)
      viewState.activeConnections.set(new Set(['git/default']))
      const empty: RuntimeState = {
        activity: { ...stamp('b', 1), state: { kind: 'empty' } },
        providers: providers('b', 1),
      }

      // Another host, but not the pair an attempt starts with: it installs nothing, and an
      // attempt is asked for, whose first pair may.
      source.feed().state(empty, false)
      expect(service.allEntries()).toEqual(activityLog.entries)
      expect(source.retries).toBe(1)

      source.feed().state(empty, true)
      expect(service.allEntries()).toEqual([])
      expect([...viewState.activeConnections()].sort()).toEqual(['git/default', 'jira/default'])
    })

    it('applies a part only past what is installed of it', () => {
      const { source, service } = connect()
      source.feed().state(pair('a', 5), true)

      source.feed().providers(providers('a', 4, [git!]))
      expect(service.providerInfo()).toHaveLength(2)
      source.feed().providers(providers('a', 6, [git!]))
      expect(service.providerInfo()).toHaveLength(1)
    })

    it('discards a pair of another authority that is no newer than what is installed', () => {
      const { source, service } = connect()
      source.feed().state(pair('a', 8), true)

      source.feed().state(pair('a', 7, [git!], '2'), false)
      expect(service.providerInfo()).toHaveLength(2)
      source.feed().state(pair('a', 9, [git!], '2'), false)
      expect(service.providerInfo()).toHaveLength(1)
    })

    it('shows every usable source once a save is installed, whichever comes first', async () => {
      const { source, service, viewState } = connect()
      source.feed().state(pair('a', 1), true)

      // The answer first: its install is not on screen yet.
      viewState.activeConnections.set(new Set(['git/default']))
      await service.saveSettings(() => Promise.resolve(saved('a', 2)))
      expect(viewState.activeConnections().has('jira/default')).toBe(false)
      source.feed().providers(providers('a', 2))
      expect(viewState.activeConnections().has('jira/default')).toBe(true)

      // The update first: the answer finds its install on screen.
      viewState.activeConnections.set(new Set(['git/default']))
      source.feed().providers(providers('a', 3, [git!, jira!]))
      await service.saveSettings(() => Promise.resolve(saved('a', 3)))
      expect(viewState.activeConnections().has('jira/default')).toBe(true)
    })

    it('keeps a save’s receipt when a check is sent after it', async () => {
      const { source, service, viewState } = connect()
      source.feed().state(pair('a', 1), true)
      viewState.activeConnections.set(new Set(['git/default']))
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      const saving = service.saveSettings(
        () => new Promise<ProviderConfigSaveResponseDto>(resolve => (answer = resolve))
      )

      await service.recheckConnection({ source: 'git', name: 'default' })
      answer(saved('a', 50))
      await saving
      source.feed().providers(providers('a', 50))

      expect(viewState.activeConnections().has('jira/default')).toBe(true)
    })

    it('never activates a receipt of a host that is gone', async () => {
      const { source, service, viewState } = connect()
      source.feed().state(pair('a', 1), true)
      // Answered by host a, which a restart replaces before its install shows.
      await service.saveSettings(() => Promise.resolve(saved('a', 9)))
      source.feed().state(pair('b', 1), true)
      viewState.activeConnections.set(new Set(['git/default']))

      source.feed().providers(providers('b', 9, [git!, jira!]))
      source.feed().providers(providers('b', 10))
      expect(viewState.activeConnections().has('jira/default')).toBe(false)

      // A receipt answered under another host than the one on screen never waits either.
      await service.saveSettings(() => Promise.resolve(saved('a', 11)))
      source.feed().providers(providers('b', 11, [git!, jira!]))
      expect(viewState.activeConnections().has('jira/default')).toBe(false)
    })

    it('never activates a save its host wrote but did not install (B70)', async () => {
      const { source, service, viewState } = connect()
      source.feed().state(pair('a', 1), true)
      viewState.activeConnections.set(new Set(['git/default']))
      const written = { ...saved('a', 1), installed: null }

      await service.saveSettings(() => Promise.resolve(written))
      source.feed().providers(providers('a', 2))

      expect(service.receiptStanding(written.installed)).toBe('notInstalled')
      expect(viewState.activeConnections().has('jira/default')).toBe(false)
    })

    it('forgets a save’s receipt once its install is on screen', async () => {
      const { source, service, viewState } = connect()
      source.feed().state(pair('a', 1), true)
      await service.saveSettings(() => Promise.resolve(saved('a', 2)))
      source.feed().providers(providers('a', 2))
      // The user hides a source after the save showed every one.
      viewState.activeConnections.set(new Set(['git/default']))

      source.feed().providers(providers('a', 3, [git!, { ...jira!, reason: 'checked again' }]))

      expect(viewState.activeConnections().has('jira/default')).toBe(false)
    })

    it('installs no answer of a command a later one superseded, but reads the host', async () => {
      const { source, backend, service } = connect()
      source.feed().state(pair('a', 1), true)
      // The host's revisions are past what is installed, so only the token can refuse it.
      backend.runtimeState()
      const first = held()
      backend.collectGate = first.gate
      backend.collectResponse = withEntries(activityLog, [])
      const superseded = service.collect('2026-05-10', '2026-05-11')
      backend.collectGate = new Promise(() => undefined)
      backend.collectResponse = collectingLog
      void service.collect('2026-05-12', '2026-05-13')
      const follows = source.follows

      first.release()
      await superseded

      expect(service.allEntries()).toEqual(activityLog.entries)
      expect(source.follows).toBe(follows + 1)
    })

    it('applies no command answer of another host than the one on screen', async () => {
      const { source, backend, service } = connect()
      source.feed().state(pair('a', 1), true)
      backend.runtimeState()
      // The backend restarted; its answer is past what is installed, but of another host.
      backend.epoch = 'b'
      backend.collectResponse = withEntries(activityLog, [])

      await service.collect('2026-05-10', '2026-05-11')

      expect(service.allEntries()).toEqual(activityLog.entries)
      expect(service.hostEpoch()).not.toBe('b')
    })

    it('binds a collect sent before any host to the first, so the next never shows its refusal', async () => {
      const { source, service } = connect()
      const post = held()
      vi.spyOn(TestBed.inject(ActivityBackendClient), 'collect').mockImplementationOnce(() =>
        post.gate.then(() => Promise.reject(new BackendFailure('internal', 'refused by a')))
      )
      const sending = service.collect('2026-05-17', '2026-05-18')
      source.feed().state(pair('a', 2), true)
      source.feed().state(pair('b', 1), true)

      post.release()
      await sending

      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('keeps a collect running as the host says, also with no connection pending', async () => {
      const { source, service } = connect()
      source.feed().state(pair('a', 1, []), true)
      const stamp = { epoch: 'a', authority: '1', revision: 2 }
      const log = withEntries(activityLog, [])
      vi.spyOn(TestBed.inject(ActivityBackendClient), 'collect').mockResolvedValueOnce({
        ...log,
        kind: 'running',
        stamp,
      })

      await service.collect('2026-05-17', '2026-05-18')
      source.feed().state(
        {
          activity: { ...stamp, state: { kind: 'running', log } },
          providers: providers('a', 1, []),
        },
        false
      )

      expect(service.collectionStatus().kind).toBe('collecting')
    })

    it('says where a save’s receipt stands against the sources on screen', () => {
      const { source, service } = connect()
      expect(service.receiptStanding({ epoch: 'a', revision: 1 })).toBe('current')
      source.feed().state(pair('a', 5), true)

      expect(service.receiptStanding({ epoch: 'a', revision: 5 })).toBe('current')
      expect(service.receiptStanding({ epoch: 'a', revision: 7 })).toBe('current')
      expect(service.receiptStanding({ epoch: 'a', revision: 4 })).toBe('superseded')
      expect(service.receiptStanding({ epoch: 'b', revision: 9 })).toBe('otherHost')
    })

    it('shows no refusal of one host on the next', async () => {
      const { source, backend, service } = connect()
      source.feed().state(pair('a', 1), true)
      backend.collectError = new BackendFailure('internal', 'refused by a')
      await service.collect('2026-05-01', '2026-05-02')
      expect(service.collectionStatus()).toMatchObject({ kind: 'failed' })

      source.feed().state(pair('b', 1), true)

      expect(service.collectionStatus()).toEqual({ kind: 'idle' })
    })

    it('asks for one resync at a time and again only for evidence the answer did not cover', () => {
      const { source, service } = connect()
      source.feed().state(pair('a', 2), true)

      // Parts of an authority not installed, past it: a pair was missed.
      source.feed().activity(activity('a', 30, '2'))
      source.feed().providers(providers('a', 31, providerInfo, '2'))
      expect(source.retries).toBe(1)
      // An ordinary pair while the resync is out asks for nothing more.
      source.feed().state(pair('a', 20, [git!], '2'), false)
      expect(source.retries).toBe(1)

      // The resync's own pair is older than the evidence: one more.
      source.feed().state(pair('a', 25, [git!], '2'), true)
      expect(source.retries).toBe(2)
      source.feed().state(pair('a', 31, [git!], '2'), true)
      expect(source.retries).toBe(2)
      expect(service.providerInfo()).toHaveLength(1)
      source.feed().activity(activity('a', 32, '2'))
      expect(source.retries).toBe(2)
    })
  })
})
