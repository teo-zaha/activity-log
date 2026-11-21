import { Service, computed, inject, type Signal } from '@angular/core'
import type { ActivityEntry, ExportOutcome, OutputFormat } from '../generated/activity'
import { localDayRange } from '../utils/local-day'
import { ActivityDataService } from './activity-data.service'
import { ActivitySelection } from './activity-selection.service'
import { ActivityViewStateService } from './activity-view-state.service'
import { ActivityBackendClient } from './backend/backend-client'
import { failure, failureWhile } from './failure'
import { NoticeService } from './notice.service'
import { ToastService } from './toast.service'

/** The range `[start, end)` an export covers, as RFC 3339 instants. */
interface ExportRange {
  readonly start: string
  readonly end: string
}

/**
 * Export of what the view on screen shows: the backend renders the file in the browser's time
 * zone. A failure is the snackbar's notice; a saved file says so.
 */
@Service()
export class ActivityExportService {
  private readonly backend = inject(ActivityBackendClient)
  private readonly data = inject(ActivityDataService)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly selection = inject(ActivitySelection)
  private readonly notices = inject(NoticeService)
  private readonly toast = inject(ToastService)

  /**
   * Whether there is a log to export: one is in and no collection is replacing it. The header's
   * buttons and the menu's items both read it.
   */
  readonly canExport: Signal<boolean> = computed(
    () => this.data.meta() !== null && !this.data.isLoading()
  )

  /**
   * Exports the entries the view shows (`ActivitySelection.exportedEntries`) over this tab's
   * local days, else the collected range.
   */
  async exportShown(format: OutputFormat): Promise<void> {
    this.notices.clear('export')
    const [from, to] = [this.viewState.dateFrom(), this.viewState.dateTo()]
    const range = from && to ? localDayRange(from, to) : this.data.meta()
    if (!range) {
      this.notices.show({
        origin: 'export',
        failure: failure('Export failed: nothing has been collected yet'),
      })
      return
    }
    try {
      const entries = this.selection.exportedEntries().map(shown => shown.entry)
      const outcome = await this.send(format, entries, range)
      if (outcome === 'saved') this.toast.success('Exported')
    } catch (error) {
      this.notices.show({ origin: 'export', failure: failureWhile('Export', error) })
    }
  }

  private send(
    format: OutputFormat,
    entries: readonly ActivityEntry[],
    range: ExportRange
  ): Promise<ExportOutcome> {
    return this.backend.export({
      format,
      start: range.start,
      end: range.end,
      timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
      entries: [...entries],
    })
  }
}
