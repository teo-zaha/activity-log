import type {
  ActivityLog,
  AddConnectionRequestDto,
  AddConnectionResponseDto,
  CollectRequest,
  ConnectionId,
  CollectionFailedResponse,
  ExportFile,
  ExportRequest,
  LogsResponse,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  ProvidersResponse,
} from '../../generated/activity'

/**
 * The log of the last collection, partial while `meta.pending` names connections it still waits
 * for, or the range and error of one that failed.
 */
export type ActivityResponse = ActivityLog | CollectionFailedResponse

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
  /** Tauri command; its payload is passed as `{ request }`. `null` marks a browser-only operation. */
  readonly tauriCommand: string | null
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
  readonly tauri: string | null
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
  addConnection: command<AddConnectionRequestDto, AddConnectionResponseDto>({
    label: 'Add source',
    tauri: 'add_connection',
  }),
  /** Runs a source's startup check again; the backend then collects its kind again. */
  recheckConnection: command<ConnectionId, ProvidersResponse>({
    label: 'Check again',
    tauri: 'recheck_connection',
    http: { method: 'POST', url: '/api/connections/recheck' },
  }),
  removeConnection: command<ConnectionId, ProviderConfigSaveResponseDto>({
    label: 'Remove source',
    tauri: 'remove_connection',
  }),
  /** Renders the export and saves it through a native dialog. */
  saveExport: command<ExportRequest, void>({ label: 'Export', tauri: 'save_export' }),
  /** Renders the export for the browser to download. */
  renderExport: command<ExportRequest, ExportFile>({
    label: 'Export',
    tauri: null,
    http: { method: 'POST', url: '/api/export' },
  }),
  /** The `activity-log:` link that launched the desktop app, once; later ones come as events. */
  takeLaunchDeepLink: command<void, string | null>({
    label: 'Deep link',
    tauri: 'take_launch_deep_link',
  }),
  /** Opens a web link in the default browser; the browser opens links itself. */
  openUrl: command<string, void>({ label: 'Open link', tauri: 'open_url' }),
  clearLogs: command<void, void>({
    label: 'Clear logs',
    tauri: 'clear_logs',
    http: { method: 'DELETE', url: '/api/logs' },
  }),
} as const

export class BrowserOnlyBackendError extends Error {
  constructor(operation: string) {
    super(`${operation} is only available in the browser.`)
    this.name = 'BrowserOnlyBackendError'
  }
}

export class DesktopOnlyBackendError extends Error {
  constructor(operation: string) {
    super(`${operation} is only available in desktop mode.`)
    this.name = 'DesktopOnlyBackendError'
  }
}
