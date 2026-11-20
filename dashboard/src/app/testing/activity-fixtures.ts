import { resource, type Provider, type ResourceRef } from '@angular/core'
import type {
  ActivityLog,
  ActivityState,
  ActivityUpdate,
  CollectAnswer,
  CollectionFailedResponse,
  PartStamp,
  ProvidersUpdate,
  RuntimeState,
  BusyStatus,
  ExportOutcome,
  ExportRequest,
  LogEntry,
  MeetingWeight,
  ProviderConfigSaveResponseDto,
  ProviderInfo,
  ProvidersResponse,
  Source,
} from '../generated/activity'
import {
  BackendOperations,
  type BackendOperation,
  type ReadOperation,
} from '../services/backend/backend-contract'
import type { ConnectionKey } from '../providers/connection'
import { BackendFailure } from '../services/backend/backend-failure'
import { BackendTransport } from '../services/backend/backend-transport'

/**
 * What the mock host shows, in the shape `GET /api/activity` has it: a log, partial while
 * `meta.pending` names connections, or a failed collection's range and error.
 */
export type ActivityResponse = ActivityLog | CollectionFailedResponse

export const providerInfo: ProviderInfo[] = [
  {
    connection: { source: 'git', name: 'default' },
    label: 'This computer',
    status: 'available',
    usable: true,
    supportedOs: ['linux', 'macos', 'windows'],
    checkedAt: '2026-05-18T08:00:00Z',
  },
  {
    connection: { source: 'jira', name: 'default' },
    label: 'Jira',
    status: 'available',
    usable: true,
    supportedOs: ['linux', 'macos', 'windows'],
    checkedAt: '2026-05-18T08:00:00Z',
  },
]

/** A source the fixtures above do not know: one added while the dashboard is open. */
export const gitlabProvider: ProviderInfo = {
  connection: { source: 'gitlab', name: 'default' },
  label: 'GitLab',
  status: 'available',
  usable: true,
  supportedOs: ['linux', 'macos', 'windows'],
  checkedAt: '2026-05-18T08:00:00Z',
}

export const providersResponse: ProvidersResponse = {
  currentOs: 'linux',
  providers: providerInfo,
}

export const activityLog: ActivityLog = {
  meta: {
    generated: '2026-05-18T08:00:00Z',
    // Local midnights, so the loaded range reads as 17–18 May in any time zone.
    start: new Date(2026, 4, 17).toISOString(),
    end: new Date(2026, 4, 19).toISOString(),
  },
  summary: {
    connections: [
      { connection: { source: 'git', name: 'default' }, count: 1 },
      { connection: { source: 'jira', name: 'default' }, count: 1 },
    ],
    mergedCount: 0,
  },
  entries: [
    {
      source: 'git',
      connection: 'default',
      id: 'abcdef123456',
      timestamp: '2026-05-18T09:00:00Z',
      repo: 'activity-log',
      hash: 'abcdef123456',
      message: 'Improve dashboard',
      author: 'Dev',
      branch: 'main',
    },
    {
      source: 'jira',
      connection: 'default',
      id: 'AL-1:updated',
      type: 'issueUpdated',
      timestamp: '2026-05-18T10:00:00Z',
      key: 'AL-1',
      title: 'Fix timeline',
      status: 'In Progress',
      issueType: 'Task',
    },
  ],
}

/** `activityLog` while its collection still runs: Git is in, Jira is pending. */
export const collectingLog: ActivityLog = {
  meta: { ...activityLog.meta, pending: [{ source: 'jira', name: 'default' }] },
  summary: {
    connections: [{ connection: { source: 'git', name: 'default' }, count: 1 }],
    mergedCount: 0,
  },
  entries: activityLog.entries.filter(entry => entry.source === 'git'),
}

/**
 * In-memory stand-in for the transport seam: the real `ActivityBackendClient` runs on top of it.
 * Tune the public fields per test.
 */
export class MockBackendTransport implements BackendTransport {
  /**
   * What the host shows at the next reads, before `activityResponse`: a `BackendFailure` fails
   * that read, and `null` is the host before its first collection.
   */
  activityResponses: (ActivityResponse | BackendFailure | null)[] = [activityLog]
  /** What the host shows at every read once `activityResponses` is used up. */
  activityResponse: ActivityResponse | null = activityLog
  providersResponse = providersResponse
  /** Fails every read while set. */
  stateError: BackendFailure | null = null
  /** When set, a read answers once it resolves, with the state as of the request. */
  stateGate: Promise<void> | null = null
  /** The host's epoch and authority, as its stamps carry them. */
  epoch = 'mock-host'
  authority = '1'
  collectResponse: ActivityResponse = activityLog
  /** Answers to the next collect requests, before `collectResponse`. */
  collectResponses: ActivityResponse[] = []
  collectError: BackendFailure | null = null
  /** When set, a collect answer waits for it; the gate is taken when the request is sent. */
  collectGate: Promise<void> | null = null
  /** When set, a settings change's answer waits for it; taken when the request is sent. */
  saveGate: Promise<void> | null = null
  recheckError: BackendFailure | null = null
  exports: ExportRequest[] = []
  /** Every link passed to `openUrl`, in order. */
  openedUrls: string[] = []
  /** Every request passed to `call`, in order. */
  requests: unknown[] = []
  logs: LogEntry[] = []
  logsError: BackendFailure | null = null
  clearLogsError: BackendFailure | null = null

  /** The host's revisions: one counter, a part taking the next value when it changes. */
  private revision = 0
  private shownActivity: ActivityUpdate | null = null
  private shownProviders: ProvidersUpdate | null = null

  resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    return resource({ loader: () => this.call(read, undefined) })
  }

  call<Req, Res>(
    operation: BackendOperation<Req, Res>,
    request: NoInfer<Req>,
    signal?: AbortSignal
  ): Promise<Res> {
    this.requests.push(request)
    // The response type is only a phantom on the operation, so an identity check in `respond`
    // cannot narrow `Res`; a typed handler map would need the same cast to store its entries.
    // Each case in `respond` returns the response its operation declares.
    const answer = this.respond(operation) as Promise<Res>
    return signal === undefined ? answer : Promise.race([answer, aborted(signal)])
  }

  saveExport(request: ExportRequest): Promise<ExportOutcome> {
    this.exports.push(request)
    return Promise.resolve('saved')
  }

  /** Hands `deepLinks` to the listener the way the desktop shell would. */
  deepLinks: string[] = []
  listenForDeepLinks(open: (link: string) => void): Promise<void> {
    for (const link of this.deepLinks) open(link)
    return Promise.resolve()
  }

  openUrl(url: string): Promise<void> {
    this.openedUrls.push(url)
    return Promise.resolve()
  }

  /** The host's state now, both parts. */
  runtimeState(): RuntimeState {
    return { activity: this.showActivity(this.activityResponse), providers: this.showProviders() }
  }

  /** Shows `response` as the host's activity: a new revision only when it changed. */
  private showActivity(response: ActivityResponse | null): ActivityUpdate {
    const state = hostStateOf(response)
    if (this.shownActivity === null || !sameJson(this.shownActivity.state, state)) {
      this.shownActivity = { ...this.stamp(), state }
    }
    return { ...this.shownActivity, epoch: this.epoch, authority: this.authority }
  }

  private showProviders(): ProvidersUpdate {
    const providers = this.providersResponse
    if (this.shownProviders === null || !sameJson(this.shownProviders.providers, providers)) {
      this.shownProviders = { ...this.stamp(), providers }
    }
    return { ...this.shownProviders, epoch: this.epoch, authority: this.authority }
  }

  private stamp(): PartStamp {
    return { epoch: this.epoch, authority: this.authority, revision: ++this.revision }
  }

  private saveResponse(): ProviderConfigSaveResponseDto {
    const providers = this.showProviders()
    return {
      snapshot: { kinds: [], connections: [] },
      providersResponse: this.providersResponse,
      installed: { epoch: providers.epoch, revision: providers.revision },
    }
  }

  private gated<T>(gate: Promise<void> | null, answer: T): Promise<T> {
    return (gate ?? Promise.resolve()).then(() => answer)
  }

  private respond(operation: BackendOperation<unknown, unknown>): Promise<unknown> {
    switch (operation) {
      case BackendOperations.runtimeState: {
        const next =
          this.activityResponses.length > 0 ? this.activityResponses.shift() : this.activityResponse
        if (this.stateError) return Promise.reject(this.stateError)
        if (next instanceof BackendFailure) return Promise.reject(next)
        const pair: RuntimeState = {
          activity: this.showActivity(next ?? null),
          providers: this.showProviders(),
        }
        return this.gated(this.stateGate, pair)
      }
      case BackendOperations.recheckConnection: {
        if (this.recheckError) return Promise.reject(this.recheckError)
        const { epoch, authority, revision } = this.showProviders()
        return Promise.resolve({ ...this.providersResponse, stamp: { epoch, authority, revision } })
      }
      case BackendOperations.saveProviderConfig:
      case BackendOperations.removeConnection:
        return this.gated(this.saveGate, this.saveResponse())
      case BackendOperations.addConnection:
        return this.gated(this.saveGate, {
          created: this.providersResponse.providers.at(-1)?.connection ?? {
            source: 'gitlab',
            name: 'default',
          },
          ...this.saveResponse(),
        })
      case BackendOperations.collect: {
        if (this.collectError) return Promise.reject(this.collectError)
        const response = this.collectResponses.shift() ?? this.collectResponse
        const { epoch, authority, revision, state } = this.showActivity(response)
        return this.gated(this.collectGate, collectAnswerOf(state, { epoch, authority, revision }))
      }
      case BackendOperations.logs:
        return this.logsError
          ? Promise.reject(this.logsError)
          : Promise.resolve({ entries: [...this.logs] })
      case BackendOperations.clearLogs:
        if (this.clearLogsError) return Promise.reject(this.clearLogsError)
        this.logs = []
        return Promise.resolve()
      default:
        return Promise.reject(
          new BackendFailure('unsupported', `${operation.label} is not implemented`)
        )
    }
  }
}

/** The state a host shows for `response`, as `ActivityState` in core puts it. */
function hostStateOf(response: ActivityResponse | null): ActivityState {
  if (response === null) return { kind: 'empty' }
  if ('error' in response) return { kind: 'failed', failure: response }
  return (response.meta.pending ?? []).length > 0
    ? { kind: 'running', log: response }
    : { kind: 'ready', log: response }
}

/** What `collect` answers for the state the host shows. */
function collectAnswerOf(state: ActivityState, stamp: PartStamp): CollectAnswer {
  switch (state.kind) {
    case 'empty':
      throw new Error('The mock host answers a collect only with a collection')
    case 'failed':
      return { ...state.failure, kind: 'failed', stamp }
    case 'running':
    case 'ready':
      return { ...state.log, kind: state.kind, stamp }
  }
}

function sameJson(a: unknown, b: unknown): boolean {
  return JSON.stringify(a) === JSON.stringify(b)
}

/** Rejects once `signal` is aborted, as a transport does. */
function aborted(signal: AbortSignal): Promise<never> {
  return new Promise((_, reject) => {
    if (signal.aborted) reject(signal.reason)
    else signal.addEventListener('abort', () => reject(signal.reason), { once: true })
  })
}

export function provideMockBackend(backend: MockBackendTransport): Provider {
  return { provide: BackendTransport, useValue: backend }
}

/** Connection keys for the default connection of each kind. */
export function connectionSet(...sources: Source[]): Set<ConnectionKey> {
  return new Set(sources.map((source): ConnectionKey => `${source}/default`))
}

/** The weight the backend sends with a busy status (core `MeetingWeight::of`), for fixtures. */
export function meetingWeightOf(status: BusyStatus | undefined): MeetingWeight {
  switch (status) {
    case 'tentative':
      return 'tentative'
    case 'free':
    case 'out_of_office':
      return 'none'
    default:
      return 'busy'
  }
}

/** What the dashboard sends to export the fixture log as Markdown. */
export const exportRequest: ExportRequest = {
  format: 'markdown',
  start: activityLog.meta.start,
  end: activityLog.meta.end,
  timeZone: 'Europe/Berlin',
  entries: activityLog.entries,
}
