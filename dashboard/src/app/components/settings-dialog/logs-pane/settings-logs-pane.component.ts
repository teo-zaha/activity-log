import {
  afterNextRender,
  Component,
  DestroyRef,
  type ElementRef,
  InjectionToken,
  computed,
  effect,
  inject,
  linkedSignal,
  signal,
  type TemplateRef,
  viewChild,
} from '@angular/core'
import { Clipboard } from '@angular/cdk/clipboard'
import { NgTemplateOutlet } from '@angular/common'
import { AutoAnimateDirective } from '../../auto-animate/auto-animate.directive'
import { interval } from '@signality/core/scheduling/interval'
import { ButtonComponent } from '../../button/button.component'
import { EmptyStateComponent } from '../../empty-state/empty-state.component'
import { FeedbackComponent } from '../../feedback/feedback.component'
import { SettingsPageActions } from '../settings-page-actions'
import {
  type SegmentedOption,
  SegmentedControlComponent,
} from '../../segmented-control/segmented-control.component'
import type { LogEntry, LogsResponse } from '../../../generated/activity'
import { ActivityBackendClient } from '../../../services/backend/backend-client'
import { BackendOperations } from '../../../services/backend/backend-contract'
import { type Failure, failureOf } from '../../../services/failure'
import {
  type LogLevelFilter,
  formatLogTime,
  formatLogsAsText,
  matchesLevelFilter,
} from '../../../utils/log-format'

export const LOGS_POLL_INTERVAL_MS = new InjectionToken<number>('LOGS_POLL_INTERVAL_MS', {
  factory: (): number => 2000,
})

/** Above this many rows the list stops animating (see `animateRows`). */
const ANIMATED_ROWS_MAX = 200

@Component({
  selector: 'app-settings-logs-pane',
  imports: [
    AutoAnimateDirective,
    ButtonComponent,
    EmptyStateComponent,
    FeedbackComponent,
    NgTemplateOutlet,
    SegmentedControlComponent,
  ],
  templateUrl: './settings-logs-pane.component.html',
  styleUrl: './settings-logs-pane.component.scss',
})
export class SettingsLogsPaneComponent {
  private readonly backend = inject(ActivityBackendClient)
  private readonly clipboard = inject(Clipboard)

  private readonly logs = this.backend.readResource(BackendOperations.logs)
  /** Shown until the next poll succeeds. */
  private readonly clearError = linkedSignal<LogsResponse | undefined, Failure | null>({
    source: () => (this.logs.hasValue() ? this.logs.value() : undefined),
    computation: () => null,
  })

  /** Keeps the last good entries on screen while a poll reloads or fails. */
  private readonly entries = linkedSignal<readonly LogEntry[] | undefined, readonly LogEntry[]>({
    source: () => (this.logs.hasValue() ? this.logs.value().entries : undefined),
    computation: (entries, previous) => entries ?? previous?.value ?? [],
  })
  protected readonly loaded = computed(() => this.logs.hasValue() || this.logs.error() != null)
  protected readonly loadError = computed(() => {
    const error = this.logs.error()
    return this.clearError() ?? (error ? failureOf(error) : null)
  })
  protected readonly copied = signal(false)
  protected readonly filter = signal<LogLevelFilter>('all')

  protected readonly filterOptions = computed((): readonly SegmentedOption<LogLevelFilter>[] => {
    const counts = this.counts()
    return [
      { value: 'all', label: 'All', count: counts.all },
      { value: 'warn', label: 'Warnings', count: counts.warn, tone: 'warn' },
      { value: 'error', label: 'Errors', count: counts.error, tone: 'error' },
    ]
  })

  private readonly filtered = computed(() =>
    this.entries().filter(entry => matchesLevelFilter(entry.level, this.filter()))
  )
  /** Newest first, which is what you want when chasing the last failure. */
  protected readonly rows = computed(() => [...this.filtered()].reverse())
  /**
   * A new line moves every row below it, and AutoAnimate measures and moves each row of the list
   * on every change. That is fine for a short log; the backend keeps up to 1000 lines.
   */
  protected readonly animateRows = computed(() => this.rows().length < ANIMATED_ROWS_MAX)
  protected readonly counts = computed(() => {
    const entries = this.entries()
    return {
      all: entries.length,
      warn: entries.filter(entry => entry.level === 'warn').length,
      error: entries.filter(entry => entry.level === 'error').length,
    }
  })

  protected readonly formatTime = formatLogTime

  private copiedTimer: ReturnType<typeof setTimeout> | undefined

  private readonly pageActions = inject(SettingsPageActions, { optional: true })
  /** Copy and Clear go in the settings page header when the pane is shown in one. */
  protected readonly inPageHeader = this.pageActions !== null
  private readonly actions = viewChild.required<TemplateRef<unknown>>('actions')
  private readonly list = viewChild<ElementRef<HTMLElement>>('list')

  constructor() {
    interval(() => this.logs.reload(), inject(LOGS_POLL_INTERVAL_MS))
    this.keepColumnsForLeavingRows()
    afterNextRender(() => this.pageActions?.template.set(this.actions()))
    inject(DestroyRef).onDestroy(() => {
      clearTimeout(this.copiedTimer)
      this.pageActions?.template.set(null)
    })
  }

  protected async clear(): Promise<void> {
    try {
      await this.backend.clearLogs()
      this.logs.set({ entries: [] })
      this.clearError.set(null)
    } catch (error) {
      const failed = failureOf(error)
      this.clearError.set(failed)
      // The pane shows the message; the log keeps what failed, as the app's toasts do.
      console.warn(failed.message, failed.cause)
    }
  }

  /**
   * AutoAnimate takes a removed row out of the flow (`position: absolute`) while it fades, and a
   * row out of the flow is no subgrid: its cells would fall into one column. The stylesheet gives
   * it the columns the rows have, from `--log-columns`, read here before the rows change: a
   * component effect runs before its template updates, so this is the layout the leaving rows had.
   */
  private keepColumnsForLeavingRows(): void {
    effect(() => {
      this.rows()
      const list = this.list()?.nativeElement
      // Narrow screens give each row columns of its own (no grid list).
      if (!list || getComputedStyle(list).display !== 'grid') return
      const row = list.querySelector('.log-row:not([style*="position: absolute"])')
      if (!row) return
      // Cells stretch to their column. The message column fills the rest, whatever the width.
      const widths = [...row.children].slice(0, 3).map(cell => cell.getBoundingClientRect().width)
      list.style.setProperty(
        '--log-columns',
        `${widths.map(w => `${w}px`).join(' ')} minmax(0, 1fr)`
      )
    })
  }

  protected copy(): void {
    if (this.clipboard.copy(formatLogsAsText(this.filtered()))) {
      this.copied.set(true)
      clearTimeout(this.copiedTimer)
      // Hand-written: Signality has no one-shot timer (only `interval`, debounce and throttle).
      this.copiedTimer = setTimeout(() => this.copied.set(false), 1600)
    }
  }
}
