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
import { BackendTransport } from '../services/backend/backend-transport'
import type { ExportRequest } from '../services/backend/export-request'

export const providerInfo: ProviderInfo[] = [
  {
    id: 'git',
    status: 'available',
    usable: true,
    supportedOs: ['linux', 'macos', 'windows'],
    checkedAt: '2026-05-18T08:00:00Z',
  },
  {
    id: 'jira',
    status: 'available',
    usable: true,
    supportedOs: ['linux', 'macos', 'windows'],
    checkedAt: '2026-05-18T08:00:00Z',
  },
]

export const providersResponse: ProvidersResponse = {
  currentOs: 'linux',
  providers: providerInfo,
}

export const activityLog: ActivityLog = {
  meta: {
    generated: '2026-05-18T08:00:00Z',
    since: '2026-05-17T00:00:00Z',
    until: '2026-05-18T23:59:59Z',
  },
  summary: {
    git_count: 1,
    gitlab_count: 0,
    jira_count: 1,
    outlook_count: 0,
    system_count: 0,
    merged_count: 0,
  },
  entries: [
    {
      source: 'git',
      timestamp: '2026-05-18T09:00:00Z',
      repo: 'activity-log',
      hash: 'abcdef123456',
      message: 'Improve dashboard',
      author: 'Dev',
      branch: 'main',
    },
    {
      source: 'jira',
      type: 'issueUpdated',
      timestamp: '2026-05-18T10:00:00Z',
      key: 'AL-1',
      title: 'Fix timeline',
      status: 'In Progress',
      issueType: 'Task',
    },
  ],
}

/**
 * In-memory stand-in for the transport seam: the real `ActivityBackendClient` runs on top of it.
 * Tune the public fields per test.
 */
export class MockBackendTransport implements BackendTransport {
  activityResponses: ActivityResponse[] = [activityLog]
  providersResponse = providersResponse
  collectResponse: ActivityResponse = activityLog
  collectError: Error | null = null
  exports: ExportRequest[] = []
  logs: LogEntry[] = []
  logsError: Error | null = null
  clearLogsError: Error | null = null

  resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    return resource({ loader: () => this.call(read, undefined) })
  }

  call<Req, Res>(operation: BackendOperation<Req, Res>, _request: NoInfer<Req>): Promise<Res> {
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
      case BackendOperations.providers:
        return Promise.resolve(this.providersResponse)
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

export function sourceSet(...sources: Source[]): Set<Source> {
  return new Set(sources)
}
