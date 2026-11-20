import { HttpClient, httpResource } from '@angular/common/http'
import { inject, resource, type ResourceRef, Service } from '@angular/core'
import { firstValueFrom, fromEvent, map, merge } from 'rxjs'
import type { ExportRequest, OutputFormat } from '../../generated/activity'
import {
  type BackendOperation,
  BackendOperations,
  DesktopOnlyBackendError,
  type ReadOperation,
} from './backend-contract'
import type { BackendTransport } from './backend-transport'
import { isWebLink } from '../../utils/text-links'

/** How long a downloaded export's object URL stays valid; FileSaver.js waits as long. */
const EXPORT_URL_LIFETIME_MS = 40_000

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

  call<Req, Res>(
    operation: BackendOperation<Req, Res>,
    request: NoInfer<Req>,
    signal?: AbortSignal
  ): Promise<Res> {
    const { http } = operation
    if (!http) return Promise.reject(new DesktopOnlyBackendError(operation.label))
    const answer = this.http.request<Res>(http.method, http.url, { body: request })
    if (signal === undefined) return firstValueFrom(answer)
    signal.throwIfAborted()
    // Unsubscribing on abort cancels the browser's request.
    const aborted = fromEvent(signal, 'abort').pipe(
      map(() => {
        throw signal.reason
      })
    )
    return firstValueFrom(merge(answer, aborted))
  }

  /** The API renders the file; the browser downloads it. */
  async saveExport(request: ExportRequest): Promise<void> {
    const file = await this.call(BackendOperations.renderExport, request)
    const blob = new Blob([file.content], { type: this.mimeTypeFor(request.format) })
    const href = URL.createObjectURL(blob)
    const link = document.createElement('a')
    link.href = href
    link.download = file.fileName
    // Firefox follows a click only on a link in the document.
    document.body.append(link)
    try {
      link.click()
    } finally {
      link.remove()
      // The browser reads the file after `click()` returns; revoking at once can cancel it. A
      // plain timer: this runs per download, outside any injection context a Signality
      // composable needs, and must outlive whatever started it.
      setTimeout(() => URL.revokeObjectURL(href), EXPORT_URL_LIFETIME_MS)
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
