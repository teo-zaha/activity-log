import type {
  ActivityLog,
  CollectRequest,
  ProviderInfo,
  ProvidersResponse,
  Source,
} from '../generated/activity'
import { ActivityBackendClient, type ActivityResponse } from '../services/backend/backend-client'
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

export class MockActivityBackendClient implements ActivityBackendClient {
  activityResponses: ActivityResponse[] = [activityLog]
  providersResponse = providersResponse
  collectResponse: ActivityResponse = activityLog
  exports: ExportRequest[] = []

  getActivity(): Promise<ActivityResponse> {
    return Promise.resolve(this.activityResponses.shift() ?? activityLog)
  }

  getProviders(): Promise<ProvidersResponse> {
    return Promise.resolve(this.providersResponse)
  }

  collect(_request: CollectRequest): Promise<ActivityResponse> {
    return Promise.resolve(this.collectResponse)
  }

  getProviderConfig(): Promise<never> {
    return Promise.reject(new Error('not implemented'))
  }

  saveProviderConfig(): Promise<never> {
    return Promise.reject(new Error('not implemented'))
  }

  export(request: ExportRequest): Promise<void> {
    this.exports.push(request)
    return Promise.resolve()
  }
}

export function sourceSet(...sources: Source[]): Set<Source> {
  return new Set(sources)
}
