import { resource, type ResourceRef, Service } from '@angular/core'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  BackendOperations,
  BrowserOnlyBackendError,
  type BackendOperation,
  type ReadOperation,
} from './backend-contract'
import type { BackendTransport } from './backend-transport'
import type { ExportRequest } from '../../generated/activity'

@Service()
export class TauriBackendTransport implements BackendTransport {
  resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    return resource({ loader: () => this.call(read, undefined) })
  }

  call<Req, Res>(
    operation: BackendOperation<Req, Res>,
    request: NoInfer<Req>,
    signal?: AbortSignal
  ): Promise<Res> {
    const command = operation.tauriCommand
    if (command === null) return Promise.reject(new BrowserOnlyBackendError(operation.label))
    const args = request === undefined ? undefined : { request }
    const answer = invoke<Res>(command, args).catch((error: unknown) => {
      throw asError(error)
    })
    // An invoke cannot be cancelled: an abort only stops the caller waiting for it.
    return signal === undefined ? answer : Promise.race([answer, abortOf(signal)])
  }

  async saveExport(request: ExportRequest): Promise<void> {
    await this.call(BackendOperations.saveExport, request)
  }

  async openUrl(url: string): Promise<void> {
    await this.call(BackendOperations.openUrl, url)
  }

  /** Listens first, so no link falls between the launch link and the first event. */
  async listenForDeepLinks(open: (link: string) => void): Promise<void> {
    await listen<string>(DEEP_LINK_EVENT, event => open(event.payload))
    const launch = await this.call(BackendOperations.takeLaunchDeepLink, undefined)
    if (launch !== null) open(launch)
  }
}

/** The app event the desktop shell sends a deep link with (`desktop/src/deep_link`). */
const DEEP_LINK_EVENT = 'deep-link'

/**
 * Rejects once `signal` is aborted, with its reason. A plain listener: it is per call, outside
 * any injection context a Signality `listener` needs, and `once` removes it.
 */
function abortOf(signal: AbortSignal): Promise<never> {
  return new Promise((_, reject) => {
    if (signal.aborted) reject(signal.reason)
    else signal.addEventListener('abort', () => reject(signal.reason), { once: true })
  })
}

/** Tauri commands reject with the Rust error as a plain string; callers and resources expect an `Error`. */
function asError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error))
}
