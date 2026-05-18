import { Injectable, computed, inject } from '@angular/core'
import type {
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  Source,
} from '../generated/activity'
import type { DayGroup } from '../models/day-group'
import type { EntryUiAdapter } from '../providers/types'
import { ActivityDataService } from './activity-data.service'
import { ActivityExportService } from './activity-export.service'
import { ActivityViewStateService } from './activity-view-state.service'
import { ProviderConfigFacade } from './provider-config-facade.service'
import type { ViewMode } from './types'

@Injectable({ providedIn: 'root' })
export class ActivityService {
  private readonly data = inject(ActivityDataService)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly exportService = inject(ActivityExportService)
  private readonly providerConfig = inject(ProviderConfigFacade)

  readonly allEntries = this.data.allEntries
  readonly allEntryAdapters = this.data.allEntryAdapters
  readonly providerInfo = this.data.providerInfo
  readonly providers = this.data.providers
  readonly summary = this.data.summary
  readonly sourceCounts = this.data.sourceCounts
  readonly meta = this.data.meta
  readonly isLoading = this.data.isLoading
  readonly error = this.data.error

  readonly dateFrom = this.viewState.dateFrom
  readonly dateTo = this.viewState.dateTo
  readonly searchQuery = this.viewState.searchQuery
  readonly activeSources = this.viewState.activeSources
  readonly currentView = this.viewState.currentView
  readonly selectedDay = this.viewState.selectedDay
  readonly selectedEntry = this.viewState.selectedEntry
  readonly selectedEntries = this.viewState.selectedEntries
  readonly hourHeight = this.viewState.hourHeight
  readonly weekDays = this.viewState.weekDays
  readonly workWeekDays = this.viewState.workWeekDays

  readonly filteredEntries = computed(() => {
    const entries = this.allEntryAdapters()
    const from = this.dateFrom()
    const to = this.dateTo()
    const query = this.currentView() === 'list' ? this.searchQuery().toLowerCase() : ''
    const sources = this.activeSources()

    return entries.filter(entry => {
      const day = entry.timestamp.slice(0, 10)
      if (from && day < from) return false
      if (to && day > to) return false
      if (!entry.sources.some(source => sources.has(source))) return false
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

  navigateDay(delta: number): void {
    this.viewState.navigateDay(delta)
  }

  navigateWeek(delta: number): void {
    this.viewState.navigateWeek(delta)
  }

  goToToday(): void {
    this.viewState.goToToday()
  }

  setView(view: ViewMode): void {
    this.viewState.setView(view)
  }

  toggleSource(source: Source): void {
    this.viewState.toggleSource(source, this.providerInfo())
  }

  isolateSource(source: Source): void {
    this.viewState.isolateSource(source, this.providerInfo())
  }

  initLoad(): Promise<void> {
    return this.data.initLoad()
  }

  collect(since: string, until: string): Promise<void> {
    return this.data.collect(since, until)
  }

  async exportJson(): Promise<void> {
    this.error.set(null)
    try {
      await this.exportService.exportJson(
        this.meta(),
        this.filteredEntries(),
        this.dateFrom(),
        this.dateTo()
      )
    } catch (error) {
      this.error.set(error instanceof Error ? error.message : 'Export failed: Unknown error')
    }
  }

  async exportMarkdown(): Promise<void> {
    this.error.set(null)
    try {
      await this.exportService.exportMarkdown(
        this.dateFrom(),
        this.dateTo(),
        this.filteredEntries(),
        this.groupedByDay()
      )
    } catch (error) {
      this.error.set(error instanceof Error ? error.message : 'Export failed: Unknown error')
    }
  }

  loadProviderConfig(): Promise<ProviderConfigSnapshotDto> {
    return this.providerConfig.loadProviderConfig()
  }

  saveProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto> {
    return this.providerConfig.saveProviderConfig(request)
  }
}
