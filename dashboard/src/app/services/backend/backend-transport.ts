import { Service, inject, type ResourceRef } from '@angular/core'
import type { BackendOperation, ReadOperation } from './backend-contract'
import type { ExportRequest } from '../../generated/activity'
import { HttpBackendTransport } from './http-backend-transport'
import { RuntimeEnvironmentService } from './runtime-environment.service'
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
  /** A read as a signal resource: `value()`, `status()`, `error()` and `reload()` come with it. */
  abstract resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined>
  /** `request` is checked against the operation, never used to infer its type. */
  abstract call<Req, Res>(
    operation: BackendOperation<Req, Res>,
    request: NoInfer<Req>
  ): Promise<Res>
  /** Saves an export: a browser download over HTTP, a native save dialog on desktop. */
  abstract saveExport(request: ExportRequest): Promise<void>
  /** Opens an http(s) link outside the app: a new browser tab, the default browser on desktop. */
  abstract openUrl(url: string): Promise<void>
  /**
   * Passes each `activity-log:` link the desktop app gets to `open`, the one that launched it
   * first. The browser gets none. The links are untrusted input.
   */
  abstract listenForDeepLinks(open: (link: string) => void): Promise<void>
}
