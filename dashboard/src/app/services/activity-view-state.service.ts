import { computed, signal, Service } from '@angular/core'
import type { ProviderInfo } from '../generated/activity'
import { type ConnectionKey, connectionKey } from '../providers/connection'
import type { EntryUiAdapter } from '../providers/types'
import type { ViewMode } from './types'
import { addDaysToKey, dateOfKey, todayKey } from '../utils/local-day'

@Service()
export class ActivityViewStateService {
  readonly dateFrom = signal('')
  readonly dateTo = signal('')
  readonly searchQuery = signal('')
  readonly activeConnections = signal(new Set<ConnectionKey>())
  readonly currentView = signal<ViewMode>('list')
  readonly selectedDay = signal<string>(todayKey())
  readonly selectedEntry = signal<EntryUiAdapter | null>(null)
  readonly selectedEntries = signal<EntryUiAdapter[] | null>(null)
  readonly hourHeight = signal(64)

  readonly weekDays = computed<string[]>(() => {
    const start = this.mondayOf(this.selectedDay())
    return Array.from({ length: 7 }, (_, i) => addDaysToKey(start, i))
  })

  readonly workWeekDays = computed<string[]>(() => this.weekDays().slice(0, 5))

  navigateDay(delta: number): void {
    this.selectedDay.set(addDaysToKey(this.selectedDay(), delta))
  }

  navigateWeek(delta: number): void {
    this.navigateDay(delta * 7)
  }

  goToToday(): void {
    this.selectedDay.set(todayKey())
  }

  setView(view: ViewMode): void {
    this.currentView.set(view)
    this.searchQuery.set('')
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

  clearSelection(): void {
    this.selectedEntry.set(null)
    this.selectedEntries.set(null)
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
