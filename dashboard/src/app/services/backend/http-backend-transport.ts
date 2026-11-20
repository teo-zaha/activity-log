import { HttpClient } from '@angular/common/http'
import { inject, resource, type ResourceRef, Service } from '@angular/core'
import { catchError, firstValueFrom, fromEvent, map, merge } from 'rxjs'
import type { ExportOutcome, ExportRequest, OutputFormat } from '../../generated/activity'
import { type BackendOperation, BackendOperations, type ReadOperation } from './backend-contract'
import { BackendFailure, httpFailureOf, unsupportedIn } from './backend-failure'
import type { BackendTransport } from './backend-transport'
import { isWebLink } from '../../utils/text-links'

/** How long a downloaded export's object URL stays valid; FileSaver.js waits as long. */
const EXPORT_URL_LIFETIME_MS = 40_000

/** Runs `work` with the browser's own failure as a `BackendFailure`, its cause kept. */
function inBrowser<T>(message: string, work: () => T): T {
  try {
    return work()
  } catch (error) {
    throw new BackendFailure('internal', message, { cause: error })
  }
}

@Service()
export class HttpBackendTransport implements BackendTransport {
  private readonly http = inject(HttpClient)

  /** Loaded through `call`, so a resource fails with the same `BackendFailure` a call does. */
  resource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    return resource({ loader: ({ abortSignal }) => this.call(read, undefined, abortSignal) })
  }

  /** Rejects with a `BackendFailure`, or with the signal's reason once the caller aborts. */
  call<Req, Res>(
    operation: BackendOperation<Req, Res>,
    request: NoInfer<Req>,
    signal?: AbortSignal
  ): Promise<Res> {
    const { http } = operation
    if (signal?.aborted) return Promise.reject(signal.reason)
    if (!http) return Promise.reject(unsupportedIn('desktop mode', operation.label))
    const answer = this.http.request<Res>(http.method, http.url, { body: request }).pipe(
      catchError((error: unknown) => {
        throw httpFailureOf(error)
      })
    )
    if (signal === undefined) return firstValueFrom(answer)
    // Unsubscribing on abort cancels the browser's request.
    const aborted = fromEvent(signal, 'abort').pipe(
      map(() => {
        throw signal.reason
      })
    )
    return firstValueFrom(merge(answer, aborted))
  }

  /** The API renders the file; the browser downloads it, so it counts as saved once handed over. */
  async saveExport(request: ExportRequest): Promise<ExportOutcome> {
    const file = await this.call(BackendOperations.renderExport, request)
    inBrowser('The browser could not download the export', () => {
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
        // The browser reads the file after `click()` returns; revoking at once can cancel it.
        // A plain timer: this runs per download, outside any injection context a Signality
        // composable needs, and must outlive whatever started it.
        setTimeout(() => URL.revokeObjectURL(href), EXPORT_URL_LIFETIME_MS)
      }
    })
    return 'saved'
  }

  async openUrl(url: string): Promise<void> {
    if (!isWebLink(url)) throw new BackendFailure('invalidRequest', 'Only web links can be opened.')
    inBrowser('The browser could not open the link', () =>
      window.open(url, '_blank', 'noopener,noreferrer')
    )
  }

  /** A browser opens `activity-log:` links in no tab of its own. */
  async listenForDeepLinks(): Promise<void> {}

  private mimeTypeFor(format: OutputFormat): string {
    return format === 'markdown' ? 'text/markdown' : 'application/json'
  }
}
