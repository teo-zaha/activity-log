import { resource, type Provider, type ResourceRef } from '@angular/core'
import type {
  ActivityLog,
  BusyStatus,
  ExportRequest,
  LogEntry,
  MeetingWeight,
  ProviderConfigSaveResponseDto,
  ProviderInfo,
  ProvidersResponse,
  Source,
} from '../generated/activity'
import {
  type ActivityResponse,
  BackendOperations,
  type BackendOperation,
  type ReadOperation,
} from '../services/backend/backend-contract'
import type { ConnectionKey } from '../providers/connection'
import { BackendTransport } from '../services/backend/backend-transport'

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
  activityResponses: ActivityResponse[] = [activityLog]
  providersResponse = providersResponse
  providersError: Error | null = null
  /** When set, a providers answer waits for it; the answer is the response as of the request. */
  providersGate: Promise<void> | null = null
  collectResponse: ActivityResponse = activityLog
  /** Answers to the next collect requests, before `collectResponse`. */
  collectResponses: ActivityResponse[] = []
  collectError: Error | null = null
  exports: ExportRequest[] = []
  /** Every link passed to `openUrl`, in order. */
  openedUrls: string[] = []
  /** Every request passed to `call`, in order. */
  requests: unknown[] = []
  logs: LogEntry[] = []
  logsError: Error | null = null
  clearLogsError: Error | null = null

  resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    return resource({ loader: () => this.call(read, undefined) })
  }

  call<Req, Res>(operation: BackendOperation<Req, Res>, request: NoInfer<Req>): Promise<Res> {
    this.requests.push(request)
    // The response type is only a phantom on the operation, so an identity check in `respond`
    // cannot narrow `Res`; a typed handler map would need the same cast to store its entries.
    // Each case in `respond` returns the response its operation declares.
    return this.respond(operation) as Promise<Res>
  }

  saveExport(request: ExportRequest): Promise<void> {
    this.exports.push(request)
    return Promise.resolve()
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

  private saveResponse(): ProviderConfigSaveResponseDto {
    return { snapshot: { kinds: [], connections: [] }, providersResponse: this.providersResponse }
  }

  private respond(operation: BackendOperation<unknown, unknown>): Promise<unknown> {
    switch (operation) {
      case BackendOperations.activity:
        return Promise.resolve(this.activityResponses.shift() ?? activityLog)
      case BackendOperations.providers: {
        if (this.providersError) return Promise.reject(this.providersError)
        const response = this.providersResponse
        return (this.providersGate ?? Promise.resolve()).then(() => response)
      }
      case BackendOperations.recheckConnection:
        return Promise.resolve(this.providersResponse)
      case BackendOperations.saveProviderConfig:
      case BackendOperations.removeConnection:
        return Promise.resolve(this.saveResponse())
      case BackendOperations.addConnection:
        return Promise.resolve({
          created: this.providersResponse.providers.at(-1)?.connection ?? {
            source: 'gitlab',
            name: 'default',
          },
          ...this.saveResponse(),
        })
      case BackendOperations.collect:
        return this.collectError
          ? Promise.reject(this.collectError)
          : Promise.resolve(this.collectResponses.shift() ?? this.collectResponse)
      case BackendOperations.logs:
        return this.logsError
          ? Promise.reject(this.logsError)
          : Promise.resolve({ entries: [...this.logs] })
      case BackendOperations.clearLogs:
        if (this.clearLogsError) return Promise.reject(this.clearLogsError)
        this.logs = []
        return Promise.resolve()
      default:
        return Promise.reject(new Error(`${operation.label} is not implemented`))
    }
  }
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
