import { Location } from '@angular/common'
import { computed, inject, InjectionToken, linkedSignal, signal, Service } from '@angular/core'
import {
  NavigationCancel,
  NavigationEnd,
  NavigationError,
  type NavigationExtras,
  NavigationSkipped,
  Router,
} from '@angular/router'
import { storage } from '@signality/core/browser/storage'
import { debounceCallback } from '@signality/core/scheduling/debounce-callback'
import { filter, firstValueFrom } from 'rxjs'
import type { SettingsSelection } from '../components/settings-dialog/settings-dialog.types'
import type { ProviderInfo } from '../generated/activity'
import { type ConnectionKey, type EntryKey, connectionKey } from '../providers/connection'
import type { ViewMode } from './types'
import {
  type AppLocation,
  type AppQueryParams,
  DEFAULT_LOCATION,
  parseAppUrl,
  settingsParam,
  viewCommands,
} from '../utils/app-url'
import { addDaysToKey, dateOfKey, todayKey } from '../utils/local-day'
import { DEFAULT_HOUR_HEIGHT, hourHeightSerializer } from '../utils/calendar-zoom'

/** How long typing in the filter pauses before the URL follows it. */
export const SEARCH_URL_DELAY_MS = new InjectionToken<number>('SEARCH_URL_DELAY_MS', {
  providedIn: 'root',
  factory: (): number => 200,
})

export interface DayRange {
  readonly from: string
  readonly to: string
}

/**
 * What the dashboard shows. The view, its day, the filter text, the range and the settings
 * section live in the URL: they are read from the router, and their setters navigate. Selection,
 * shown sources and zoom are kept here.
 */
@Service()
export class ActivityViewStateService {
  private readonly router = inject(Router)
  private readonly location = inject(Location)

  /** The URL on screen, as the router last showed it. */
  private readonly url = computed<AppLocation>(() => {
    const url = this.router.lastSuccessfulNavigation()?.finalUrl
    return (url && parseAppUrl(url)) ?? DEFAULT_LOCATION
  })
  /** The range the last collection covered; the URL's range wins over it. */
  private readonly collectedRange = signal<DayRange>({ from: '', to: '' })

  readonly currentView = computed<ViewMode>(() => this.url().view)
  /** The calendar views' day; the timeline keeps the one shown last. */
  readonly selectedDay = linkedSignal<string | null, string>({
    source: () => this.url().day,
    computation: (day, previous) => day ?? previous?.value ?? todayKey(),
  }).asReadonly()
  /**
   * The filter text typed but not in the URL yet: the list follows each keystroke, the URL only
   * once typing pauses. Dropped when the view changes, since the text belongs to its view.
   */
  private readonly typedQuery = linkedSignal<ViewMode, string | null>({
    // The view, not the URL: other navigations keep what is being typed.
    source: this.currentView,
    computation: () => null,
  })
  readonly searchQuery = computed(() => this.typedQuery() ?? this.url().q)
  private readonly commitQueryLater = debounceCallback(
    () => void this.commitQuery(),
    inject(SEARCH_URL_DELAY_MS)
  )
  readonly dateFrom = computed(() => this.url().from ?? this.collectedRange().from)
  readonly dateTo = computed(() => this.url().to ?? this.collectedRange().to)
  /** The settings section open over the view, or null. */
  readonly settings = computed<SettingsSelection | null>(() => this.url().settings)

  readonly activeConnections = signal(new Set<ConnectionKey>())
  /** The event whose details the URL opens; in a group, the one open in it. */
  readonly openEventKey = computed<EntryKey | null>(() => this.url().event)
  /** The overlapping events the URL lists together; empty without a group. */
  readonly openGroupKeys = computed<readonly EntryKey[]>(() => this.url().events)
  readonly hourHeight = storage('calendar-hour-height', DEFAULT_HOUR_HEIGHT, {
    serializer: hourHeightSerializer,
  })

  readonly weekDays = computed<string[]>(() => {
    const start = this.mondayOf(this.selectedDay())
    return Array.from({ length: 7 }, (_, i) => addDaysToKey(start, i))
  })

  readonly workWeekDays = computed<string[]>(() => this.weekDays().slice(0, 5))

  /**
   * The range of the URL the dashboard was opened with, if it names both days. Read from the
   * browser's address, since the router has not shown the first URL yet while the app starts.
   */
  openedRange(): DayRange | null {
    const opened = parseAppUrl(this.router.parseUrl(this.location.path()))
    return opened?.from && opened.to ? { from: opened.from, to: opened.to } : null
  }

  /** Shows the range a collection covered, unless the URL names one. */
  showCollectedRange(range: DayRange): void {
    this.collectedRange.set(range)
  }

  navigateDay(delta: number): void {
    this.showDay(addDaysToKey(this.selectedDay(), delta))
  }

  navigateWeek(delta: number): void {
    this.navigateDay(delta * 7)
  }

  goToToday(): void {
    this.showDay(todayKey())
  }

  /** Switches the view; the filter text belongs to the view it was typed in. */
  setView(view: ViewMode): Promise<boolean> {
    this.typedQuery.set(null)
    return this.navigate(viewCommands(view, this.selectedDay()), { q: null })
  }

  /** Opens the day view on `day`. */
  openDay(day: string): Promise<boolean> {
    this.typedQuery.set(null)
    return this.navigate(viewCommands('day', day), { q: null })
  }

  /** Filters at once; the URL follows once typing pauses, see `typedQuery`. */
  setSearchQuery(query: string): void {
    this.typedQuery.set(query)
    this.commitQueryLater()
  }

  /** Puts the typed filter text into the URL, in place of the current entry. */
  private async commitQuery(): Promise<void> {
    const query = this.typedQuery()
    if (query === null) return
    const shown = await this.navigate([], { q: query || null }, true)
    // Text typed meanwhile stays until its own commit.
    if (shown && this.typedQuery() === query) this.typedQuery.set(null)
  }

  setDateFrom(day: string): Promise<boolean> {
    return this.setRange(day, this.dateTo())
  }

  setDateTo(day: string): Promise<boolean> {
    return this.setRange(this.dateFrom(), day)
  }

  // ── Settings ───────────────────────────────────────────────────────────────
  // Like the event details, each navigation records how many history entries the settings took
  // (`settingsDepth`): closing goes back past all of them, and replaces the URL where that still
  // shows the settings (a link opened them) or where they took no entry.

  /** Opens the settings on `selection` over the view. */
  openSettings(selection: SettingsSelection): Promise<boolean> {
    return this.navigateSettings(selection, 1)
  }

  /** Moves the open settings to another section in place, as the list beside it does. */
  showSettings(selection: SettingsSelection): Promise<boolean> {
    return this.navigateSettings(selection, this.historyDepth('settingsDepth'), true)
  }

  /** Opens a section as a page of its own from the settings home; Back returns to the list. */
  enterSettings(selection: SettingsSelection): Promise<boolean> {
    return this.navigateSettings(selection, this.historyDepth('settingsDepth') + 1)
  }

  /** From a section page back to the settings home. */
  backToSettingsHome(): Promise<boolean> {
    return this.goBack(
      this.historyDepth('settingsDepth') > 1 ? 1 : 0,
      () => this.navigateSettings('home', this.historyDepth('settingsDepth'), true),
      location => location.settings !== null && location.settings !== 'home'
    )
  }

  /** Closes the settings, back to the URL they were opened from. */
  closeSettings(): Promise<boolean> {
    return this.goBack(
      this.historyDepth('settingsDepth'),
      () => this.navigateSettings(null, 0, true),
      location => location.settings !== null
    )
  }

  private navigateSettings(
    selection: SettingsSelection | null,
    settingsDepth: number,
    replace = false
  ): Promise<boolean> {
    return this.router.navigate([], {
      queryParams: { settings: selection === null ? null : settingsParam(selection) },
      replaceUrl: replace,
      state: { settingsDepth },
    })
  }

  /** A history entry, unless `replace`: views, days and settings are; typing and ranges are not. */
  private navigate(
    commands: string[],
    queryParams: AppQueryParams,
    replace = false
  ): Promise<boolean> {
    const extras: NavigationExtras = { queryParams, replaceUrl: replace }
    return this.router.navigate(commands, extras)
  }

  private showDay(day: string): void {
    const view = this.currentView()
    void this.navigate(viewCommands(view === 'list' ? 'day' : view, day), {})
  }

  private setRange(from: string, to: string): Promise<boolean> {
    return this.navigate([], { from: from || null, to: to || null }, true)
  }

  toggleConnection(key: ConnectionKey, providers: readonly ProviderInfo[]): void {
    if (!this.providerInfoFor(key, providers)?.usable) return
    const current = new Set(this.activeConnections())
    if (current.has(key)) current.delete(key)
    else current.add(key)
    this.activeConnections.set(current)
  }

  isolateConnection(key: ConnectionKey, providers: readonly ProviderInfo[]): void {
    if (!this.providerInfoFor(key, providers)?.usable) return
    const current = this.activeConnections()
    if (current.size === 1 && current.has(key)) {
      this.activateAllUsable(providers)
      return
    }

    this.activeConnections.set(new Set<ConnectionKey>([key]))
  }

  /** Shows every source that is usable or may still become so, see `shownKeys`. */
  activateAllUsable(providers: readonly ProviderInfo[]): void {
    this.activeConnections.set(new Set(shownKeys(providers)))
  }

  /**
   * Moves from the `previous` list of sources to `providers`, keeping the user's choices: sources
   * that are gone or whose check failed are dropped, new ones (and ones usable again) are shown,
   * and what the user switched off stays off.
   */
  followProviders(providers: readonly ProviderInfo[], previous: readonly ProviderInfo[]): void {
    const before = new Set(shownKeys(previous))
    const active = this.activeConnections()
    this.activeConnections.set(
      new Set(shownKeys(providers).filter(key => active.has(key) || !before.has(key)))
    )
  }

  // ── Event details ──────────────────────────────────────────────────────────
  // Each navigation that opens details further records how many history entries the details
  // took (`detailDepth`), so closing them goes back past all of them. Details opened from a link
  // or a reload took none: closing them replaces the URL, as it does where going back lands on
  // the link's entry.

  /** Opens one event's details. */
  openEvent(key: EntryKey): Promise<boolean> {
    return this.navigateDetail({ event: key, events: null }, 1)
  }

  /** Lists overlapping events together. */
  openGroup(keys: readonly EntryKey[]): Promise<boolean> {
    return this.navigateDetail({ events: keys, event: null }, 1)
  }

  /** Opens one event of the listed group; Back returns to the list. */
  showInGroup(key: EntryKey): Promise<boolean> {
    return this.navigateDetail({ event: key }, this.detailDepth() + 1)
  }

  /** Moves to another event of the group in place, without a history entry. */
  stepInGroup(key: EntryKey): Promise<boolean> {
    return this.navigateDetail({ event: key }, this.detailDepth(), true)
  }

  /** From an event of the group back to its list. */
  backToGroupList(): Promise<boolean> {
    return this.goBack(
      this.detailDepth() > 1 ? 1 : 0,
      () => this.navigateDetail({ event: null }, this.detailDepth(), true),
      location => location.event !== null
    )
  }

  /** Closes the details, back to the URL they were opened from. */
  closeDetail(): Promise<boolean> {
    return this.goBack(
      this.detailDepth(),
      () => this.navigateDetail({ event: null, events: null }, 0, true),
      location => location.event !== null || location.events.length > 0
    )
  }

  private leaving: Promise<boolean> | null = null

  /**
   * Goes back `steps` entries, then replaces the entry it lands on if that still `shows` what is
   * being left: going back past details or settings opened from a link lands on the link. While
   * that runs, another Back or Close is the same one, so a double click goes back once.
   */
  private goBack(
    steps: number,
    replace: () => Promise<boolean>,
    shows: (location: AppLocation) => boolean
  ): Promise<boolean> {
    this.leaving ??= this.traverse(steps)
      .then(() => (shows(this.shownLocation()) ? replace() : true))
      .finally(() => (this.leaving = null))
    return this.leaving
  }

  private async traverse(steps: number): Promise<void> {
    if (steps === 0) return
    const settled = firstValueFrom(
      this.router.events.pipe(
        filter(
          event =>
            event instanceof NavigationEnd ||
            event instanceof NavigationCancel ||
            event instanceof NavigationError ||
            event instanceof NavigationSkipped
        )
      )
    )
    this.location.historyGo(-steps)
    await settled
  }

  /** The URL the router shows now, also between a navigation's end and the next render. */
  private shownLocation(): AppLocation {
    return parseAppUrl(this.router.parseUrl(this.router.url)) ?? DEFAULT_LOCATION
  }

  private detailDepth(): number {
    return this.historyDepth('detailDepth')
  }

  /** How many history entries the details or settings on screen took, as the entry records it. */
  private historyDepth(key: 'detailDepth' | 'settingsDepth'): number {
    const state: unknown = this.location.getState()
    if (typeof state !== 'object' || state === null) return 0
    const depth: unknown = Object.entries(state).find(([name]) => name === key)?.[1]
    return typeof depth === 'number' && depth > 0 ? depth : 0
  }

  private navigateDetail(
    queryParams: AppQueryParams,
    detailDepth: number,
    replace = false
  ): Promise<boolean> {
    return this.router.navigate([], { queryParams, replaceUrl: replace, state: { detailDepth } })
  }

  private providerInfoFor(
    key: ConnectionKey,
    providers: readonly ProviderInfo[]
  ): ProviderInfo | undefined {
    return providers.find(provider => connectionKey(provider.connection) === key)
  }

  private mondayOf(dateKey: string): string {
    const weekDay = dateOfKey(dateKey).getDay()
    return addDaysToKey(dateKey, -(weekDay === 0 ? 6 : weekDay - 1))
  }
}

/**
 * Usable sources and those still being checked: a collection reads a source as soon as its own
 * check passes, so its entries can arrive before the dashboard hears that the check is done.
 */
function shownKeys(providers: readonly ProviderInfo[]): ConnectionKey[] {
  return providers
    .filter(provider => provider.usable || provider.status === 'checking')
    .map(provider => connectionKey(provider.connection))
}
