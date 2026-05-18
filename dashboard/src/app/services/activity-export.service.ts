import { Injectable, inject } from '@angular/core'
import type { ActivityLog } from '../generated/activity'
import type { DayGroup } from '../models/day-group'
import type { ExportedActivityLog } from '../models/exported-activity-log'
import type { EntryUiAdapter } from '../providers/types'
import { ActivityBackendClient } from './backend/backend-client'
import { extractBackendErrorMessage } from './backend/error-message'

@Injectable({ providedIn: 'root' })
export class ActivityExportService {
  private readonly backend = inject(ActivityBackendClient)

  async exportJson(
    meta: ActivityLog['meta'] | null,
    entries: readonly EntryUiAdapter[],
    dateFrom: string,
    dateTo: string
  ): Promise<void> {
    try {
      const payload: ExportedActivityLog = {
        meta,
        entries: entries.map(entry => entry.entry),
      }
      await this.backend.export({
        content: JSON.stringify(payload, null, 2),
        fileName: `${this.exportFileNameBase(dateFrom, dateTo)}.json`,
        format: 'json',
      })
    } catch (error) {
      throw new Error(`Export failed: ${extractBackendErrorMessage(error)}`)
    }
  }

  async exportMarkdown(
    dateFrom: string,
    dateTo: string,
    entries: readonly EntryUiAdapter[],
    groupedByDay: readonly DayGroup<EntryUiAdapter>[]
  ): Promise<void> {
    try {
      const from = dateFrom || 'all'
      const to = dateTo || 'all'
      let markdown = `# Activity Summary - ${from} to ${to}\n\nTotal: ${entries.length} events\n\n`

      for (const { date, entries: dayEntries } of groupedByDay) {
        const label = new Date(date + 'T12:00:00Z').toLocaleDateString('en-GB', {
          weekday: 'long',
          year: 'numeric',
          month: 'long',
          day: 'numeric',
        })
        markdown += `## ${label}\n\n`
        for (const entry of dayEntries) {
          const time = new Date(entry.timestamp).toLocaleTimeString('en-GB', {
            hour: '2-digit',
            minute: '2-digit',
          })
          markdown += `- **[${entry.source}]** ${entry.markdownSummary()} (${entry.markdownMeta(time)})\n`
        }
        markdown += '\n'
      }

      await this.backend.export({
        content: markdown,
        fileName: `${this.exportFileNameBase(dateFrom, dateTo)}.md`,
        format: 'markdown',
      })
    } catch (error) {
      throw new Error(`Export failed: ${extractBackendErrorMessage(error)}`)
    }
  }

  private exportFileNameBase(dateFrom: string, dateTo: string): string {
    return `activity-${dateFrom || 'all'}-${dateTo || 'all'}`
  }
}
