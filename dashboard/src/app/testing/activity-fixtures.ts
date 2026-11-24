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
  GitHubEntry,
  FigmaEntry,
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
import type { RuntimeStream, RuntimeStreamHandlers } from '../services/backend/runtime-stream'

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
    recheckable: false,
    supportedOs: ['linux', 'macos', 'windows'],
    checkedAt: '2026-05-18T08:00:00Z',
  },
  {
    connection: { source: 'jira', name: 'default' },
    label: 'Jira',
    status: 'available',
    usable: true,
    recheckable: false,
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
  recheckable: false,
  supportedOs: ['linux', 'macos', 'windows'],
  checkedAt: '2026-05-18T08:00:00Z',
}

export const providersResponse: ProvidersResponse = {
  currentOs: 'linux',
  providers: providerInfo,
  secretStore: { store: 'os', state: 'ready' },
}

const figmaFile = {
  source: 'figma' as const,
  connection: 'design',
  timestamp: '2026-05-18T11:00:00Z',
  fileKey: 'FILEkey0001',
  fileName: 'Checkout',
  fileUrl: 'https://www.figma.com/file/FILEkey0001',
}

export const figmaEntries = {
  versionSaved: {
    ...figmaFile,
    id: 'FILEkey0001:version:7',
    type: 'versionSaved',
    editor: 'figma',
    versionId: '7',
    label: 'Checkout v2',
    description: 'Updated form labels',
  },
  edited: { ...figmaFile, id: 'FILEkey0001:version:8', type: 'edited', versionId: '8' },
  commented: {
    ...figmaFile,
    id: 'FILEkey0001:comment:9',
    type: 'commented',
    editor: 'figma',
    commentId: '9',
    message: 'Looks good',
    nodeId: '3:4',
    resolved: true,
    fileUrl: 'https://www.figma.com/file/FILEkey0001?node-id=3%3A4',
  },
  replied: {
    ...figmaFile,
    id: 'FILEkey0001:comment:10',
    type: 'replied',
    editor: 'figma',
    commentId: '10',
    parentId: '9',
    nodeId: '3:4',
    fileUrl: 'https://www.figma.com/file/FILEkey0001?node-id=3%3A4',
  },
} satisfies Record<FigmaEntry['type'], FigmaEntry>

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

export const githubPullRequestEntry = {
  source: 'github',
  connection: 'oss',
  id: 'PR_example',
  timestamp: '2026-05-18T10:30:00Z',
  repository: 'acme/widgets',
  type: 'opened',
  target: {
    kind: 'pullRequest',
    number: 12,
    title: 'Add widget',
    webUrl: 'https://github.com/acme/widgets/pull/12',
  },
} satisfies GitHubEntry

/** `activityLog` while its collection still runs: Git is in, Jira is pending. */
export const collectingLog: ActivityLog = {
  meta: { ...activityLog.meta, pending: [{ source: 'jira', name: 'default' }] },
  summary: {
    connections: [{ connection: { source: 'git', name: 'default' }, count: 1 }],
    mergedCount: 0,
  },
  entries: activityLog.entries.filter(entry => entry.source === 'git'),
}

/** A stream the mock host pushes to, and what it pushed last. */
interface MockStream {
  readonly handlers: RuntimeStreamHandlers
  open: boolean
  epoch: string
  authority: string
  activity: number
  providers: number
}

/**
 * In-memory stand-in for the transport seam: the real `ActivityBackendClient` runs on top of it,
 * and it is a host that pushes its state. A stream starts with a pair; then, every `stepMs` while
 * the host is busy (a state still queued, a collection running, a source being checked, sources
 * with nothing collected yet) and once after each command, it moves on and pushes each part that
 * changed. Tune the public fields per test.
 */
export class MockBackendTransport implements BackendTransport {
  /**
   * What the host shows at its next steps (a stream's first pair is one), before
   * `activityResponse`: a `BackendFailure` drops the streams with it, and `null` is the host
   * before its first collection.
   */
  activityResponses: (ActivityResponse | BackendFailure | null)[] = [activityLog]
  /** What the host shows once `activityResponses` is used up. */
  activityResponse: ActivityResponse | null = activityLog
  providersResponse = providersResponse
  /** While set, no stream opens, and the next step drops the open ones with it. */
  stateError: BackendFailure | null = null
  /** When set, a new stream's first pair waits for it, with the state as of opening. */
  stateGate: Promise<void> | null = null
  /** How often a busy host moves on. */
  stepMs = 1
  /** How many streams were opened. */
  streamsOpened = 0
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
  private readonly streams = new Set<MockStream>()
  private stepping: ReturnType<typeof setTimeout> | null = null

  /** A stream of this host: a pair first (after the current turn), then what each step changes. */
  openRuntimeStream(handlers: RuntimeStreamHandlers): RuntimeStream {
    this.streamsOpened++
    const stream: MockStream = {
      handlers,
      open: true,
      epoch: this.epoch,
      authority: this.authority,
      activity: 0,
      providers: 0,
    }
    void this.startStream(stream)
    return {
      close: (): void => {
        stream.open = false
        this.streams.delete(stream)
      },
    }
  }

  private async startStream(stream: MockStream): Promise<void> {
    await Promise.resolve()
    if (!stream.open) return
    if (this.stateError) return this.drop(stream, this.stateError)
    const next = this.nextShown()
    if (next instanceof BackendFailure) return this.drop(stream, next)
    const pair: RuntimeState = {
      activity: this.showActivity(next),
      providers: this.showProviders(),
    }
    if (this.stateGate) await this.stateGate
    if (!stream.open) return
    this.streams.add(stream)
    this.send(stream, pair)
    this.keepGoing()
  }

  /**
   * The host moves on once soon, whether or not it is busy: a command, or a test standing in
   * for one, changed something it pushes.
   */
  publish(): void {
    this.stepping ??= setTimeout(() => this.step(), this.stepMs)
  }

  private keepGoing(): void {
    if (this.busy()) this.publish()
  }

  private step(): void {
    this.stepping = null
    if (this.streams.size === 0) return
    const next = this.stateError ?? this.nextShown()
    if (next instanceof BackendFailure) {
      for (const stream of [...this.streams]) this.drop(stream, next)
      return
    }
    const activity = this.showActivity(next)
    const providers = this.showProviders()
    for (const stream of [...this.streams]) {
      if (stream.epoch !== this.epoch) {
        // Another host now: this one's connection is gone.
        this.drop(stream, new BackendFailure('unreachable', 'The backend restarted'))
      } else if (stream.authority !== this.authority) {
        this.send(stream, { activity, providers })
      } else {
        if (activity.revision > stream.activity) {
          stream.activity = activity.revision
          stream.handlers.activity(activity)
        }
        if (stream.open && providers.revision > stream.providers) {
          stream.providers = providers.revision
          stream.handlers.providers(providers)
        }
      }
    }
    this.keepGoing()
  }

  private send(stream: MockStream, pair: RuntimeState): void {
    stream.epoch = pair.activity.epoch
    stream.authority = pair.activity.authority
    stream.activity = pair.activity.revision
    stream.providers = pair.providers.revision
    stream.handlers.state(pair)
  }

  private drop(stream: MockStream, failure: BackendFailure): void {
    stream.open = false
    this.streams.delete(stream)
    stream.handlers.lost(failure)
  }

  private nextShown(): ActivityResponse | BackendFailure | null {
    const next =
      this.activityResponses.length > 0 ? this.activityResponses.shift() : this.activityResponse
    return next ?? null
  }

  /** Whether the host will change by itself: the dashboard is told when it does. */
  private busy(): boolean {
    if (this.activityResponses.length > 0) return true
    const providers = this.providersResponse.providers
    if (providers.some(provider => provider.status === 'checking')) return true
    const state = this.shownActivity?.state
    if (state === undefined || state.kind === 'empty') return providers.length > 0
    return state.kind === 'running'
  }

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
      snapshot: { revision: 'mock', kinds: [], connections: [] },
      providersResponse: this.providersResponse,
      installed: { epoch: providers.epoch, revision: providers.revision },
    }
  }

  /** `answer` once `gate` opens; the host moves on after a command either way. */
  private gated<T>(gate: Promise<void> | null, answer: T): Promise<T> {
    this.publish()
    return (gate ?? Promise.resolve()).then(() => answer)
  }

  private respond(operation: BackendOperation<unknown, unknown>): Promise<unknown> {
    switch (operation) {
      case BackendOperations.runtimeState: {
        if (this.stateError) return Promise.reject(this.stateError)
        const pair: RuntimeState = this.runtimeState()
        return (this.stateGate ?? Promise.resolve()).then(() => pair)
      }
      case BackendOperations.recheckConnection: {
        this.publish()
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
        this.publish()
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
