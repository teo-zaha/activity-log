import { HttpClient, httpResource } from '@angular/common/http'
import { inject, resource, type ResourceRef, Service } from '@angular/core'
import { firstValueFrom } from 'rxjs'
import {
  type BackendOperation,
  DesktopOnlyBackendError,
  type ReadOperation,
} from './backend-contract'
import type { BackendTransport } from './backend-transport'
import type { ExportRequest } from './export-request'

@Service()
export class HttpBackendTransport implements BackendTransport {
  private readonly http = inject(HttpClient)

  resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    const { http } = read
    if (!http) {
      return resource<Res, unknown>({
        loader: () => Promise.reject(new DesktopOnlyBackendError(read.label)),
      })
    }
    return httpResource<Res>(() => http.url)
  }

  call<Req, Res>(operation: BackendOperation<Req, Res>, request: NoInfer<Req>): Promise<Res> {
    const { http } = operation
    if (!http) return Promise.reject(new DesktopOnlyBackendError(operation.label))
    return firstValueFrom(this.http.request<Res>(http.method, http.url, { body: request }))
  }

  async saveExport(request: ExportRequest): Promise<void> {
    const blob = new Blob([request.content], { type: this.mimeTypeFor(request.format) })
    const href = URL.createObjectURL(blob)

    try {
      const link = document.createElement('a')
      link.href = href
      link.download = request.fileName
      link.click()
    } finally {
      URL.revokeObjectURL(href)
    }
  }

  private mimeTypeFor(format: ExportRequest['format']): string {
    return format === 'markdown' ? 'text/markdown' : 'application/json'
  }
}
