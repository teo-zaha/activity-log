import { HttpErrorResponse } from '@angular/common/http'
import { Injectable, computed, inject, signal } from '@angular/core'
import type {
  ActivityEntry,
  ActivityLog,
  CollectedActivitySummary,
  CollectingResponse,
  ErrorResponse,
  ProviderInfo,
  Source,
} from '../generated/activity'
import type { DayGroup } from '../models/day-group'
import type { ExportedActivityLog } from '../models/exported-activity-log'
import { createEntryAdapter, getProviderAdapter } from '../providers/provider-adapter'
import type { EntryUiAdapter } from '../providers/types'
import { ActivityBackendClient, type ActivityResponse } from './backend/backend-client'
import type { ProviderViewModel, ViewMode } from './types'

@Injectable({ providedIn: 'root' })
export class ActivityService {
  private readonly backend = inject(ActivityBackendClient)

  readonly allEntries = signal<ActivityEntry[]>([])
  readonly allEntryAdapters = computed(() => this.allEntries().map(createEntryAdapter))
  readonly providerInfo = signal<ProviderInfo[]>([])
  readonly providers = computed<ProviderViewModel[]>(() =>
    this.providerInfo()
      .filter(info => info.usable)
      .map(info => {
        const adapter = getProviderAdapter(info.id)
        return {
          info,
          label: adapter.label,
          icon: adapter.providerIcon,
          chipKey: info.id,
          tooltip: adapter.getStatusTooltip(info),
          count: this.sourceCounts().get(info.id) ?? 0,
        }
      })
  )
  readonly summary = signal<CollectedActivitySummary | null>(null)
  readonly sourceCounts = computed<Map<Source, number>>(() => {
    const s = this.summary()
    if (!s) return new Map()
    return new Map<Source, number>([
      ['git', s.git_count],
      ['gitlab', s.gitlab_count],
      ['jira', s.jira_count],
      ['outlook', s.outlook_count],
      ['system', s.system_count],
    ])
  })
  readonly meta = signal<ActivityLog['meta'] | null>(null)
  readonly dateFrom = signal('')
  readonly dateTo = signal('')
  readonly searchQuery = signal('')
  readonly activeSources = signal(new Set<Source>())
  readonly isLoading = signal(false)
  readonly error = signal<string | null>(null)

  // View state
  readonly currentView = signal<ViewMode>('list')
  readonly selectedDay = signal<string>(this.todayStr())
  readonly selectedEntry = signal<EntryUiAdapter | null>(null)
  readonly selectedEntries = signal<EntryUiAdapter[] | null>(null)
  readonly hourHeight = signal(64)

  // ── Computed: filtered list ───────────────────────────────────────────────

  readonly filteredEntries = computed(() => {
    const entries = this.allEntryAdapters()
    const from = this.dateFrom()
    const to = this.dateTo()
    const query = this.currentView() === 'list' ? this.searchQuery().toLowerCase() : ''
    const sources = this.activeSources()

    return entries.filter(entry => {
      const ts = entry.timestamp.slice(0, 10)
      if (from && ts < from) return false
      if (to && ts > to) return false
      if (!entry.sources.some(source => sources.has(source))) {
        return false
      }
      if (query) {
        const haystack = entry.searchTerms.join(' ').toLowerCase()
        if (!haystack.includes(query)) return false
      }
      return true
    })
  })

  readonly groupedByDay = computed<DayGroup<EntryUiAdapter>[]>(() => {
    const byDay = new Map<string, EntryUiAdapter[]>()
    for (const entry of this.filteredEntries()) {
      const day = entry.timestamp.slice(0, 10)
      if (!byDay.has(day)) byDay.set(day, [])
      byDay.get(day)!.push(entry)
    }
    return Array.from(byDay.entries())
      .sort(([a], [b]) => b.localeCompare(a))
      .map(([date, entries]) => ({ date, entries }))
  })

  // ── Computed: calendar views ──────────────────────────────────────────────

  readonly weekDays = computed<string[]>(() => {
    const start = this.mondayOf(this.selectedDay())
    return Array.from({ length: 7 }, (_, i) => {
      const d = new Date(start + 'T12:00:00')
      d.setDate(d.getDate() + i)
      return d.toISOString().slice(0, 10)
    })
  })

  readonly workWeekDays = computed<string[]>(() => this.weekDays().slice(0, 5))

  readonly dayEntries = computed(() =>
    this.filteredEntries().filter(entry => entry.timestamp.slice(0, 10) === this.selectedDay())
  )

  readonly weekEntriesByDay = computed<Map<string, EntryUiAdapter[]>>(() => {
    const days = new Set(this.weekDays())
    const map = new Map<string, EntryUiAdapter[]>()
    for (const day of days) map.set(day, [])
    for (const entry of this.filteredEntries()) {
      const day = entry.timestamp.slice(0, 10)
      if (map.has(day)) map.get(day)!.push(entry)
    }
    return map
  })

  // ── Navigation ────────────────────────────────────────────────────────────

  navigateDay(delta: number): void {
    const d = new Date(this.selectedDay() + 'T12:00:00')
    d.setDate(d.getDate() + delta)
    this.selectedDay.set(d.toISOString().slice(0, 10))
  }

  navigateWeek(delta: number): void {
    this.navigateDay(delta * 7)
  }

  goToToday(): void {
    this.selectedDay.set(this.todayStr())
  }

  // ── Data loading ──────────────────────────────────────────────────────────

  loadLog(log: ActivityLog): void {
    this.allEntries.set(log.entries)
    this.selectedEntry.set(null)
    this.selectedEntries.set(null)
    this.summary.set(log.summary)
    this.meta.set(log.meta)
    if (log.meta.since) this.dateFrom.set(log.meta.since.slice(0, 10))
    if (log.meta.until) this.dateTo.set(log.meta.until.slice(0, 10))
  }

  setView(view: ViewMode): void {
    this.currentView.set(view)
    this.searchQuery.set('')
  }

  toggleSource(source: Source): void {
    if (!this.providerInfoFor(source)?.usable) return
    const current = new Set(this.activeSources())
    if (current.has(source)) current.delete(source)
    else current.add(source)
    this.activeSources.set(current)
  }

  isolateSource(source: string): void {
    const current = this.activeSources()
    if (current.size === 1 && current.has(source)) {
      this.activeSources.set(new Set(['git', 'gitlab', 'jira', 'outlook', 'system']))
    } else {
      this.activeSources.set(new Set([source]))
    }
  }

  async initLoad(): Promise<void> {
    this.isLoading.set(true)
    this.error.set(null)
    try {
      await this.loadProviders()
      const log = await this.waitForActivity()
      if (log) {
        this.loadLog(log)
        return
      }
      this.error.set('Timed out waiting for initial collection.')
    } catch (e) {
      this.error.set(`Failed to load: ${this.extractErrorMessage(e)}`)
    } finally {
      this.isLoading.set(false)
    }
  }

  async collect(since: string, until: string): Promise<void> {
    this.isLoading.set(true)
    this.error.set(null)
    try {
      const response = await this.backend.collect({ since, until })
      if (this.isCollectingResponse(response)) {
        const log = await this.waitForActivity()
        if (!log) {
          this.error.set('Timed out waiting for collection.')
          return
        }
        this.loadLog(log)
        return
      }

      this.loadLog(response)
    } catch (e) {
      this.error.set(`Collection failed: ${this.extractErrorMessage(e)}`)
    } finally {
      this.isLoading.set(false)
    }
  }

  exportJson(): string {
    const payload: ExportedActivityLog = {
      meta: this.meta(),
      entries: this.filteredEntries().map(entry => entry.entry),
    }
    return JSON.stringify(payload, null, 2)
  }

  exportMarkdown(): string {
    const from = this.dateFrom() || 'all'
    const to = this.dateTo() || 'all'
    let md = `# Activity Summary — ${from} to ${to}\n\nTotal: ${this.filteredEntries().length} events\n\n`
    for (const { date, entries } of this.groupedByDay()) {
      const label = new Date(date + 'T12:00:00Z').toLocaleDateString('en-GB', {
        weekday: 'long',
        year: 'numeric',
        month: 'long',
        day: 'numeric',
      })
      md += `## ${label}\n\n`
      for (const entry of entries) {
        const t = new Date(entry.timestamp).toLocaleTimeString('en-GB', {
          hour: '2-digit',
          minute: '2-digit',
        })
        const meta = entry.markdownMeta(t)
        md += `- **[${entry.source}]** ${entry.markdownSummary()} (${meta})\n`
      }
      md += '\n'
    }
    return md
  }

  // ── Helpers ───────────────────────────────────────────────────────────────

  private todayStr(): string {
    return new Date().toISOString().slice(0, 10)
  }

  private async loadProviders(): Promise<void> {
    const response = await this.backend.getProviders()
    this.providerInfo.set(response.providers)
    this.activeSources.set(
      new Set(response.providers.filter(provider => provider.usable).map(provider => provider.id))
    )
  }

  private async waitForActivity(): Promise<ActivityLog | null> {
    for (let i = 0; i < 60; i++) {
      const response = await this.backend.getActivity()
      if (this.isCollectingResponse(response)) {
        await new Promise(resolve => setTimeout(resolve, 1000))
        continue
      }
      return response
    }

    return null
  }

  private providerInfoFor(source: Source): ProviderInfo | undefined {
    return this.providerInfo().find(provider => provider.id === source)
  }

  private isCollectingResponse(response: ActivityResponse): response is CollectingResponse {
    return 'collecting' in response
  }

  private extractErrorMessage(e: unknown): string {
    if (e instanceof HttpErrorResponse) {
      const body: unknown = e.error
      if (
        typeof body === 'object' &&
        body !== null &&
        'error' in body &&
        typeof (body as ErrorResponse).error === 'string'
      ) {
        return (body as ErrorResponse).error
      }
      if (typeof body === 'string') return body
      return e.statusText || e.message
    }
    if (e instanceof Error) return e.message
    return 'Unknown error'
  }

  private mondayOf(dateStr: string): string {
    const d = new Date(dateStr + 'T12:00:00')
    const day = d.getDay()
    d.setDate(d.getDate() - (day === 0 ? 6 : day - 1))
    return d.toISOString().slice(0, 10)
  }
}
