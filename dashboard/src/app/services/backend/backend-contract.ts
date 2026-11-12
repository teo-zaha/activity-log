import type {
  ActivityLog,
  CollectRequest,
  CollectingResponse,
  LogsResponse,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  ProvidersResponse,
} from '../../generated/activity'
import type { ExportRequest } from './export-request'

export type ActivityResponse = ActivityLog | CollectingResponse

export type HttpMethod = 'GET' | 'POST' | 'DELETE'

export interface HttpEndpoint<Method extends HttpMethod = HttpMethod> {
  readonly method: Method
  readonly url: string
}

/**
 * One backend operation, declared once. `request` and `response` exist only in the type system:
 * they let a transport call be typed from the operation alone.
 */
export interface BackendOperation<Req, Res> {
  /** Names the operation in errors. */
  readonly label: string
  /** Tauri command; its payload is passed as `{ request }`. */
  readonly tauriCommand: string
  /** `null` marks a desktop-only operation. */
  readonly http: HttpEndpoint | null
  readonly request?: Req
  readonly response?: Res
}

/** An operation without a request, served by `GET`; usable as a signal resource. */
export interface ReadOperation<Res> extends BackendOperation<void, Res> {
  readonly http: HttpEndpoint<'GET'> | null
}

interface OperationSpec {
  readonly label: string
  readonly tauri: string
}

function read<Res>(spec: OperationSpec & { readonly route?: string }): ReadOperation<Res> {
  return {
    label: spec.label,
    tauriCommand: spec.tauri,
    http: spec.route ? { method: 'GET', url: spec.route } : null,
  }
}

function command<Req, Res>(
  spec: OperationSpec & { readonly http?: HttpEndpoint }
): BackendOperation<Req, Res> {
  return { label: spec.label, tauriCommand: spec.tauri, http: spec.http ?? null }
}

export const BackendOperations = {
  activity: read<ActivityResponse>({
    label: 'Activity',
    tauri: 'get_activity',
    route: '/api/activity',
  }),
  providers: read<ProvidersResponse>({
    label: 'Providers',
    tauri: 'get_providers',
    route: '/api/providers',
  }),
  providerConfig: read<ProviderConfigSnapshotDto>({
    label: 'Provider configuration',
    tauri: 'get_provider_config',
  }),
  logs: read<LogsResponse>({ label: 'Logs', tauri: 'get_logs', route: '/api/logs' }),
  collect: command<CollectRequest, ActivityResponse>({
    label: 'Collect',
    tauri: 'collect',
    http: { method: 'POST', url: '/api/collect' },
  }),
  saveProviderConfig: command<ProviderConfigSaveRequestDto, ProviderConfigSaveResponseDto>({
    label: 'Provider configuration',
    tauri: 'save_provider_config',
  }),
  saveExport: command<ExportRequest, void>({ label: 'Export', tauri: 'save_export' }),
  clearLogs: command<void, void>({
    label: 'Clear logs',
    tauri: 'clear_logs',
    http: { method: 'DELETE', url: '/api/logs' },
  }),
} as const

export class DesktopOnlyBackendError extends Error {
  constructor(operation: string) {
    super(`${operation} is only available in desktop mode.`)
    this.name = 'DesktopOnlyBackendError'
  }
}
