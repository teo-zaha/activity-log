import type {
  AddConnectionRequestDto,
  AddConnectionResponseDto,
  CollectAnswer,
  CollectRequest,
  ConnectionId,
  ExportFile,
  ExportOutcome,
  ExportRequest,
  LogsResponse,
  ProviderConfigFieldIssueDto,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  RemoveConnectionRequestDto,
  RecheckAnswer,
  RuntimeState,
} from '../../generated/activity'

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
  /**
   * Both parts of the host's state, as one coherent pair with the host's stamps: the desktop
   * app's handshake before its events. The browser follows `GET /api/events` instead.
   */
  runtimeState: read<RuntimeState>({ label: 'Runtime state', tauri: 'get_runtime_state' }),
  providerConfig: read<ProviderConfigSnapshotDto>({
    label: 'Provider configuration',
    tauri: 'get_provider_config',
  }),
  logs: read<LogsResponse>({ label: 'Logs', tauri: 'get_logs', route: '/api/logs' }),
  collect: command<CollectRequest, CollectAnswer>({
    label: 'Collect',
    tauri: 'collect',
    http: { method: 'POST', url: '/api/collect' },
  }),
  saveProviderConfig: command<ProviderConfigSaveRequestDto, ProviderConfigSaveResponseDto>({
    label: 'Provider configuration',
    tauri: 'save_provider_config',
  }),
  /** The issues a save would be refused for, at their fields; nothing is saved. */
  checkProviderConfig: command<ProviderConfigSaveRequestDto, ProviderConfigFieldIssueDto[]>({
    label: 'Check settings',
    tauri: 'check_provider_config',
  }),
  addConnection: command<AddConnectionRequestDto, AddConnectionResponseDto>({
    label: 'Add source',
    tauri: 'add_connection',
  }),
  /** Runs a source's startup check again; the backend then collects its kind again. */
  recheckConnection: command<ConnectionId, RecheckAnswer>({
    label: 'Check again',
    tauri: 'recheck_connection',
    http: { method: 'POST', url: '/api/connections/recheck' },
  }),
  removeConnection: command<RemoveConnectionRequestDto, ProviderConfigSaveResponseDto>({
    label: 'Remove source',
    tauri: 'remove_connection',
  }),
  /** Renders the export and saves it through a native dialog. */
  saveExport: command<ExportRequest, ExportOutcome>({ label: 'Export', tauri: 'save_export' }),
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
