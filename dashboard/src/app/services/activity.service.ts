import { Service, computed, inject, signal } from '@angular/core'
import type {
  AddConnectionRequestDto,
  AddConnectionResponseDto,
  ConnectionId,
  InstallReceipt,
  OutputFormat,
  ProviderConfigFieldIssueDto,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  RemoveConnectionRequestDto,
} from '../generated/activity'
import type { SettingsSelection } from '../components/settings-dialog/settings-dialog.types'
import type { ConnectionKey, EntryKey } from '../providers/connection'
import type { DayGroup } from '../models/day-group'
import type { EntryUiAdapter } from '../providers/types'
import { ActivityDataService } from './activity-data.service'
import { failure, failureWhile, type Notice } from './failure'
import { ActivityExportService } from './activity-export.service'
import { ActivityViewStateService } from './activity-view-state.service'
import { ProviderConfigFacade } from './provider-config-facade.service'
import { ToastService } from './toast.service'
import type { ReceiptStanding } from './types'
import type { ViewMode } from '../utils/app-url'
import { localDayRange, type DayKey } from '../utils/local-day'

@Service()
export class ActivityService {
  private readonly data = inject(ActivityDataService)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly exportService = inject(ActivityExportService)
  private readonly providerConfig = inject(ProviderConfigFacade)
  private readonly toast = inject(ToastService)

  readonly allEntries = this.data.allEntries
  readonly allEntryAdapters = this.data.allEntryAdapters
  readonly providerInfo = this.data.providerInfo
  readonly providers = this.data.providers
  readonly providersLoaded = this.data.providersLoaded
  readonly unusableProviders = this.data.unusableProviders
  readonly hasNoSources = this.data.hasNoSources
  readonly summary = this.data.summary
  readonly connectionCounts = this.data.connectionCounts
  readonly meta = this.data.meta
  readonly isLoading = this.data.isLoading
  /** Whether there is a log to export: one is in and no collection is replacing it. */
  readonly canExport = computed(() => this.meta() !== null && !this.isLoading())
  /** The snackbar's notice: a failure of an action that has no place of its own to show it. */
  readonly notice = signal<Notice | null>(null)
  readonly collectionStatus = this.data.collectionStatus
  readonly connection = this.data.connection

  readonly dateFrom = this.viewState.dateFrom
  readonly dateTo = this.viewState.dateTo
  /** The range the collection on screen covers, which may differ from this tab's own. */
  readonly collectedRange = this.viewState.collectedRange
  readonly searchQuery = this.viewState.searchQuery
  readonly activeConnections = this.viewState.activeConnections
  readonly currentView = this.viewState.currentView
  readonly selectedDay = this.viewState.selectedDay
  readonly openEventKey = this.viewState.openEventKey
  readonly openGroupKeys = this.viewState.openGroupKeys
  readonly detailsOpen = this.viewState.detailsOpen
  readonly hourHeight = this.viewState.hourHeight
  readonly weekDays = this.viewState.weekDays
  readonly settings = this.viewState.settings
  readonly workWeekDays = this.viewState.workWeekDays

  /** Every entry by its `trackKey`, for the details the URL names. */
  private readonly entryByKey = computed(
    () => new Map(this.allEntryAdapters().map(entry => [entry.trackKey, entry]))
  )
  /** The event whose details are open, once its entry is loaded. */
  readonly selectedEntry = computed<EntryUiAdapter | null>(() => {
    const key = this.openEventKey()
    return key === null ? null : (this.entryByKey().get(key) ?? null)
  })
  /** The listed group's events that are loaded; null without a group. */
  readonly selectedEntries = computed<readonly EntryUiAdapter[] | null>(() => {
    const keys = this.openGroupKeys()
    if (keys.length === 0) return null
    const byKey = this.entryByKey()
    return keys.flatMap(key => byKey.get(key) ?? [])
  })

  /** Entries of the connections the source filter shows. */
  private readonly shownEntries = computed(() => {
    const active = this.activeConnections()
    return this.allEntryAdapters().filter(entry => entry.connections.some(key => active.has(key)))
  })

  readonly filteredEntries = computed(() => {
    const from = this.dateFrom()
    const to = this.dateTo()
    const query = this.currentView() === 'list' ? this.searchQuery().toLowerCase() : ''

    return this.shownEntries().filter(entry => {
      const day = entry.dayKey
      if (from && day < from) return false
      if (to && day > to) return false
      if (query && !entry.searchText.includes(query)) return false
      return true
    })
  })

  /**
   * The entries the calendar views draw: like `filteredEntries`, but a meeting that starts before
   * the range and runs into it stays, for its blocks and meeting time on the days it covers.
   */
  readonly calendarEntries = computed(() => {
    const from = this.dateFrom()
    const to = this.dateTo()
    return this.shownEntries().filter(
      entry => (!from || entry.lastDayKey >= from) && (!to || entry.dayKey <= to)
    )
  })

  readonly groupedByDay = computed<DayGroup<EntryUiAdapter>[]>(() => {
    const byDay = new Map<DayKey, EntryUiAdapter[]>()
    for (const entry of this.filteredEntries()) {
      const day = entry.dayKey
      if (!byDay.has(day)) byDay.set(day, [])
      byDay.get(day)!.push(entry)
    }
    return Array.from(byDay.entries())
      .sort(([a], [b]) => b.localeCompare(a))
      .map(([date, entries]) => ({ date, entries }))
  })

  readonly dayEntries = computed(() =>
    this.filteredEntries().filter(entry => entry.dayKey === this.selectedDay())
  )

  readonly weekEntriesByDay = computed<Map<string, EntryUiAdapter[]>>(() => {
    const days = new Set(this.weekDays())
    const map = new Map<string, EntryUiAdapter[]>()
    for (const day of days) map.set(day, [])
    for (const entry of this.filteredEntries()) {
      const day = entry.dayKey
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

  openEvent(key: EntryKey): Promise<boolean> {
    return this.viewState.openEvent(key)
  }

  openGroup(keys: readonly EntryKey[]): Promise<boolean> {
    return this.viewState.openGroup(keys)
  }

  showInGroup(key: EntryKey): Promise<boolean> {
    return this.viewState.showInGroup(key)
  }

  stepInGroup(key: EntryKey): Promise<boolean> {
    return this.viewState.stepInGroup(key)
  }

  backToGroupList(): Promise<boolean> {
    return this.viewState.backToGroupList()
  }

  closeDetail(): Promise<boolean> {
    return this.viewState.closeDetail()
  }

  setView(view: ViewMode): Promise<boolean> {
    return this.viewState.setView(view)
  }

  openDay(day: DayKey): Promise<boolean> {
    return this.viewState.openDay(day)
  }

  setSearchQuery(query: string): void {
    this.viewState.setSearchQuery(query)
  }

  setDateFrom(day: DayKey | null): Promise<boolean> {
    return this.viewState.setDateFrom(day)
  }

  setDateTo(day: DayKey | null): Promise<boolean> {
    return this.viewState.setDateTo(day)
  }

  /** The range fields are edited, or no longer: the URL's range stays while they are. */
  setRangeEditing(editing: boolean): void {
    this.viewState.setRangeEditing(editing)
  }

  /** Moves the open settings to `selection` in place. */
  showSettings(selection: SettingsSelection): Promise<boolean> {
    return this.viewState.showSettings(selection)
  }

  /** Opens `selection` as a page of its own from the settings home. */
  enterSettings(selection: SettingsSelection): Promise<boolean> {
    return this.viewState.enterSettings(selection)
  }

  backToSettingsHome(): Promise<boolean> {
    return this.viewState.backToSettingsHome()
  }

  closeSettings(): Promise<boolean> {
    return this.viewState.closeSettings()
  }

  toggleConnection(key: ConnectionKey): void {
    this.viewState.toggleConnection(key, this.providerInfo())
  }

  isolateConnection(key: ConnectionKey): void {
    this.viewState.isolateConnection(key, this.providerInfo())
  }

  initLoad(): Promise<void> {
    return this.data.initLoad()
  }

  /** Collects the local days `fromKey` to `toKey`, inclusive. */
  collect(fromKey: DayKey, toKey: DayKey): Promise<void> {
    return this.data.collect(fromKey, toKey)
  }

  /** Collects the range on screen (Collect, ⌘R); null when it has no range to collect. */
  collectShownRange(): Promise<void> | null {
    const [from, to] = [this.dateFrom(), this.dateTo()]
    return from !== null && to !== null ? this.collect(from, to) : null
  }

  /** Retry for the collection failure shown, see `ActivityDataService.retryCollection`. */
  retryCollection(): Promise<void> {
    return this.data.retryCollection()
  }

  dismissCollectionStatus(): void {
    this.data.dismissCollectionStatus()
  }

  exportJson(): Promise<void> {
    return this.exportAs('json')
  }

  exportMarkdown(): Promise<void> {
    return this.exportAs('markdown')
  }

  /**
   * The entries the view shows, over the chosen local days or else the collected range: the
   * timeline's list, or what a calendar view draws, a meeting that runs into the range included.
   */
  private async exportAs(format: OutputFormat): Promise<void> {
    if (this.notice()?.origin === 'export') this.notice.set(null)
    const from = this.dateFrom()
    const to = this.dateTo()
    const meta = this.meta()
    const range = from && to ? localDayRange(from, to) : meta
    if (!range) {
      this.notice.set({
        origin: 'export',
        failure: failure('Export failed: nothing has been collected yet'),
      })
      return
    }
    try {
      const shown = this.currentView() === 'list' ? this.filteredEntries() : this.calendarEntries()
      const outcome = await this.exportService.export(
        format,
        shown.map(entry => entry.entry),
        range
      )
      if (outcome === 'saved') this.toast.success('Exported')
    } catch (error) {
      this.notice.set({ origin: 'export', failure: failureWhile('Export', error) })
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

  checkProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigFieldIssueDto[]> {
    return this.providerConfig.checkProviderConfig(request)
  }

  addConnection(request: AddConnectionRequestDto): Promise<AddConnectionResponseDto> {
    return this.providerConfig.addConnection(request)
  }

  removeConnection(request: RemoveConnectionRequestDto): Promise<ProviderConfigSaveResponseDto> {
    return this.providerConfig.removeConnection(request)
  }

  /** Where a save's receipt stands against the state on screen, see `ReceiptStanding`. */
  receiptStanding(receipt: InstallReceipt | null): ReceiptStanding {
    return this.data.receiptStanding(receipt)
  }

  /** The host on screen, null before any. */
  readonly hostEpoch = this.data.hostEpoch

  /** Checks a source again and collects it again; a refused request rejects, for its caller. */
  recheckConnection(connection: ConnectionId): Promise<void> {
    return this.data.recheckConnection(connection)
  }
}
