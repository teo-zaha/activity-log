import { Service, inject, type ResourceRef } from '@angular/core'
import type { BackendOperation, ReadOperation } from './backend-contract'
import type { ExportOutcome, ExportRequest } from '../../generated/activity'
import { HttpBackendTransport } from './http-backend-transport'
import { RuntimeEnvironmentService } from './runtime-environment.service'
import type { RuntimeStream, RuntimeStreamHandlers } from './runtime-stream'
import { TauriBackendTransport } from './tauri-backend-transport'

/**
 * How operations reach the backend: the desktop shell invokes Tauri commands, the browser calls
 * the HTTP API. Resolves to the Tauri transport in the desktop shell and to HTTP otherwise.
 */
@Service({
  factory: (): BackendTransport =>
    inject(RuntimeEnvironmentService).isTauri()
      ? inject(TauriBackendTransport)
      : inject(HttpBackendTransport),
})
export abstract class BackendTransport {
  /**
   * A read as a signal resource: `value()`, `status()`, `error()` and `reload()` come with it.
   * Every call and resource fails with a `BackendFailure`.
   */
  abstract resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined>
  /**
   * `request` is checked against the operation, never used to infer its type. Aborting `signal`
   * rejects the call and, where the transport can, cancels the request itself.
   */
  abstract call<Req, Res>(
    operation: BackendOperation<Req, Res>,
    request: NoInfer<Req>,
    signal?: AbortSignal
  ): Promise<Res>
  /**
   * Saves an export: a browser download over HTTP, a native save dialog on desktop, which the
   * user may close without saving.
   */
  abstract saveExport(request: ExportRequest): Promise<ExportOutcome>
  /** Opens an http(s) link outside the app: a new browser tab, the default browser on desktop. */
  abstract openUrl(url: string): Promise<void>
  /**
   * Opens one connection to the host's state (`docs/plans/shared-contracts.md` §5): a `state`
   * pair first, then parts and later pairs in the order they come, until `close` or `lost`.
   * Reconnecting is the caller's (`PushStateSource`).
   */
  abstract openRuntimeStream(handlers: RuntimeStreamHandlers): RuntimeStream
  /**
   * Passes each `activity-log:` link the desktop app gets to `open`, the one that launched it
   * first. The browser gets none. The links are untrusted input.
   */
  abstract listenForDeepLinks(open: (link: string) => void): Promise<void>
}
