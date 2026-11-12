import { resource, type ResourceRef, Service } from '@angular/core'
import { invoke } from '@tauri-apps/api/core'
import { BackendOperations, type BackendOperation, type ReadOperation } from './backend-contract'
import type { BackendTransport } from './backend-transport'
import type { ExportRequest } from './export-request'

@Service()
export class TauriBackendTransport implements BackendTransport {
  resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    return resource({ loader: () => this.call(read, undefined) })
  }

  call<Req, Res>(operation: BackendOperation<Req, Res>, request: NoInfer<Req>): Promise<Res> {
    const args = request === undefined ? undefined : { request }
    return invoke<Res>(operation.tauriCommand, args).catch((error: unknown) => {
      throw asError(error)
    })
  }

  async saveExport(request: ExportRequest): Promise<void> {
    await this.call(BackendOperations.saveExport, request)
  }
}

/** Tauri commands reject with the Rust error as a plain string; callers and resources expect an `Error`. */
function asError(error: unknown): Error {
  return error instanceof Error ? error : new Error(String(error))
}
