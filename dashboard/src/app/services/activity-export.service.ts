import { inject, Service } from '@angular/core'
import type { ActivityEntry, OutputFormat } from '../generated/activity'
import { ActivityBackendClient } from './backend/backend-client'
import { extractBackendErrorMessage } from './backend/error-message'

/** The range `[start, end)` an export covers, as RFC 3339 instants. */
export interface ExportRange {
  readonly start: string
  readonly end: string
}

/** Hands the shown entries to the backend, which renders the file in the browser's time zone. */
@Service()
export class ActivityExportService {
  private readonly backend = inject(ActivityBackendClient)

  async export(
    format: OutputFormat,
    entries: readonly ActivityEntry[],
    range: ExportRange
  ): Promise<void> {
    try {
      await this.backend.export({
        format,
        start: range.start,
        end: range.end,
        timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        entries: [...entries],
      })
    } catch (error) {
      throw new Error(`Export failed: ${extractBackendErrorMessage(error)}`)
    }
  }
}
