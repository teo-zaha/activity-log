import { HttpClient, httpResource } from '@angular/common/http'
import { inject, resource, type ResourceRef, Service } from '@angular/core'
import { firstValueFrom } from 'rxjs'
import type { ExportRequest, OutputFormat } from '../../generated/activity'
import {
  type BackendOperation,
  BackendOperations,
  DesktopOnlyBackendError,
  type ReadOperation,
} from './backend-contract'
import type { BackendTransport } from './backend-transport'
import { isWebLink } from '../../utils/text-links'

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

  /** The API renders the file; the browser downloads it. */
  async saveExport(request: ExportRequest): Promise<void> {
    const file = await this.call(BackendOperations.renderExport, request)
    const blob = new Blob([file.content], { type: this.mimeTypeFor(request.format) })
    const href = URL.createObjectURL(blob)

    try {
      const link = document.createElement('a')
      link.href = href
      link.download = file.fileName
      link.click()
    } finally {
      URL.revokeObjectURL(href)
    }
  }

  async openUrl(url: string): Promise<void> {
    if (!isWebLink(url)) throw new Error('Only web links can be opened.')
    window.open(url, '_blank', 'noopener,noreferrer')
  }

  /** A browser opens `activity-log:` links in no tab of its own. */
  async listenForDeepLinks(): Promise<void> {}

  private mimeTypeFor(format: OutputFormat): string {
    return format === 'markdown' ? 'text/markdown' : 'application/json'
  }
}
