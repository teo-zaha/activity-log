import { resource, type Provider, type ResourceRef } from '@angular/core'
import type {
  ActivityLog,
  LogEntry,
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
import type { ExportRequest } from '../services/backend/export-request'

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
  collectError: Error | null = null
  exports: ExportRequest[] = []
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

  private respond(operation: BackendOperation<unknown, unknown>): Promise<unknown> {
    switch (operation) {
      case BackendOperations.activity:
        return Promise.resolve(this.activityResponses.shift() ?? activityLog)
      case BackendOperations.providers: {
        if (this.providersError) return Promise.reject(this.providersError)
        const response = this.providersResponse
        return (this.providersGate ?? Promise.resolve()).then(() => response)
      }
      case BackendOperations.saveProviderConfig:
      case BackendOperations.addConnection:
      case BackendOperations.removeConnection:
        return Promise.resolve({
          snapshot: { kinds: [], connections: [] },
          providersResponse: this.providersResponse,
        })
      case BackendOperations.collect:
        return this.collectError
          ? Promise.reject(this.collectError)
          : Promise.resolve(this.collectResponse)
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
