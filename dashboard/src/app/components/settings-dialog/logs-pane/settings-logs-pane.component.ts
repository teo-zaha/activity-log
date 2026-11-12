import {
  Component,
  DestroyRef,
  InjectionToken,
  computed,
  inject,
  linkedSignal,
  signal,
} from '@angular/core'
import { Clipboard } from '@angular/cdk/clipboard'
import { MatIconModule } from '@angular/material/icon'
import type { LogEntry, LogsResponse } from '../../../generated/activity'
import { ActivityBackendClient } from '../../../services/backend/backend-client'
import { BackendOperations } from '../../../services/backend/backend-contract'
import { extractBackendErrorMessage } from '../../../services/backend/error-message'
import {
  type LogLevelFilter,
  formatLogTime,
  formatLogsAsText,
  matchesLevelFilter,
} from '../../../utils/log-format'

export const LOGS_POLL_INTERVAL_MS = new InjectionToken<number>('LOGS_POLL_INTERVAL_MS', {
  factory: (): number => 2000,
})

interface LevelFilterOption {
  readonly id: LogLevelFilter
  readonly label: string
}

@Component({
  selector: 'app-settings-logs-pane',
  imports: [MatIconModule],
  templateUrl: './settings-logs-pane.component.html',
  styleUrl: './settings-logs-pane.component.scss',
})
export class SettingsLogsPaneComponent {
  private readonly backend = inject(ActivityBackendClient)
  private readonly clipboard = inject(Clipboard)

  private readonly logs = this.backend.readResource(BackendOperations.logs)
  /** Shown until the next poll succeeds. */
  private readonly clearError = linkedSignal<LogsResponse | undefined, string | null>({
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
    return this.clearError() ?? (error ? extractBackendErrorMessage(error) : null)
  })
  protected readonly copied = signal(false)
  protected readonly filter = signal<LogLevelFilter>('all')

  protected readonly filterOptions: readonly LevelFilterOption[] = [
    { id: 'all', label: 'All' },
    { id: 'warn', label: 'Warnings' },
    { id: 'error', label: 'Errors' },
  ]

  private readonly filtered = computed(() =>
    this.entries().filter(entry => matchesLevelFilter(entry.level, this.filter()))
  )
  /** Newest first, which is what you want when chasing the last failure. */
  protected readonly rows = computed(() => [...this.filtered()].reverse())
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

  constructor() {
    const pollTimer = setInterval(() => this.logs.reload(), inject(LOGS_POLL_INTERVAL_MS))
    inject(DestroyRef).onDestroy(() => {
      clearInterval(pollTimer)
      clearTimeout(this.copiedTimer)
    })
  }

  protected countFor(filter: LogLevelFilter): number {
    return this.counts()[filter]
  }

  protected async clear(): Promise<void> {
    try {
      await this.backend.clearLogs()
      this.logs.set({ entries: [] })
      this.clearError.set(null)
    } catch (error) {
      this.clearError.set(extractBackendErrorMessage(error))
    }
  }

  protected copy(): void {
    if (this.clipboard.copy(formatLogsAsText(this.filtered()))) {
      this.copied.set(true)
      clearTimeout(this.copiedTimer)
      this.copiedTimer = setTimeout(() => this.copied.set(false), 1600)
    }
  }
}
