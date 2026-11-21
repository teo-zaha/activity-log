import { Service, inject } from '@angular/core'
import type {
  AddConnectionRequestDto,
  AddConnectionResponseDto,
  ConnectionId,
  InstallReceipt,
  ProviderConfigFieldIssueDto,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  RemoveConnectionRequestDto,
} from '../generated/activity'
import type { SettingsSelection } from '../components/settings-dialog/settings-dialog.types'
import type { ConnectionKey, EntryKey } from '../providers/connection'
import { ActivityDataService } from './activity-data.service'
import { ActivitySelection } from './activity-selection.service'
import { ActivityExportService } from './activity-export.service'
import { ActivityViewStateService } from './activity-view-state.service'
import { ConnectionSettings } from './connection-settings.service'
import { NoticeService } from './notice.service'
import type { ReceiptStanding } from './types'
import type { ViewMode } from '../utils/app-url'
import type { DayKey } from '../utils/local-day'

/**
 * The facade the dashboard's components still inject: only delegation to `ActivityDataService`,
 * `ActivityViewStateService`, `ActivitySelection`, `ActivityExportService`, `ConnectionSettings`
 * and `NoticeService`. Consumers move to those one by one; it is deleted once none is left.
 */
@Service()
export class ActivityService {
  private readonly data = inject(ActivityDataService)
  private readonly selection = inject(ActivitySelection)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly exportService = inject(ActivityExportService)
  private readonly connectionSettings = inject(ConnectionSettings)
  private readonly notices = inject(NoticeService)

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
  readonly canExport = this.exportService.canExport
  /** The notice service's own signal, so `notice.set(…)` still shows a notice. */
  readonly notice = this.notices.current
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

  readonly selectedEntry = this.selection.selectedEntry
  readonly selectedEntries = this.selection.selectedEntries
  readonly filteredEntries = this.selection.filteredEntries
  readonly calendarEntries = this.selection.calendarEntries
  readonly groupedByDay = this.selection.groupedByDay

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
    this.selection.toggleConnection(key)
  }

  isolateConnection(key: ConnectionKey): void {
    this.selection.isolateConnection(key)
  }

  initLoad(): Promise<void> {
    return this.data.initLoad()
  }

  /** Collects the local days `fromKey` to `toKey`, inclusive. */
  collect(fromKey: DayKey, toKey: DayKey): Promise<void> {
    return this.data.collect(fromKey, toKey)
  }

  collectShownRange(): Promise<void> | null {
    return this.data.collectShownRange()
  }

  /** Retry for the collection failure shown, see `ActivityDataService.retryCollection`. */
  retryCollection(): Promise<void> {
    return this.data.retryCollection()
  }

  dismissCollectionStatus(): void {
    this.data.dismissCollectionStatus()
  }

  exportJson(): Promise<void> {
    return this.exportService.exportShown('json')
  }

  exportMarkdown(): Promise<void> {
    return this.exportService.exportShown('markdown')
  }

  loadProviderConfig(): Promise<ProviderConfigSnapshotDto> {
    return this.connectionSettings.loadProviderConfig()
  }

  saveProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto> {
    return this.connectionSettings.saveProviderConfig(request)
  }

  checkProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigFieldIssueDto[]> {
    return this.connectionSettings.checkProviderConfig(request)
  }

  addConnection(request: AddConnectionRequestDto): Promise<AddConnectionResponseDto> {
    return this.connectionSettings.addConnection(request)
  }

  removeConnection(request: RemoveConnectionRequestDto): Promise<ProviderConfigSaveResponseDto> {
    return this.connectionSettings.removeConnection(request)
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
