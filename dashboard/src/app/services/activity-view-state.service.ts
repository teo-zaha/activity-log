import { computed, signal, Service } from '@angular/core'
import type { ProviderInfo, Source } from '../generated/activity'
import type { EntryUiAdapter } from '../providers/types'
import type { ViewMode } from './types'
import { addDaysToKey, dateOfKey, todayKey } from '../utils/local-day'

@Service()
export class ActivityViewStateService {
  readonly dateFrom = signal('')
  readonly dateTo = signal('')
  readonly searchQuery = signal('')
  readonly activeSources = signal(new Set<Source>())
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

  toggleSource(source: Source, providers: readonly ProviderInfo[]): void {
    if (!this.providerInfoFor(source, providers)?.usable) return
    const current = new Set(this.activeSources())
    if (current.has(source)) current.delete(source)
    else current.add(source)
    this.activeSources.set(current)
  }

  isolateSource(source: Source, providers: readonly ProviderInfo[]): void {
    if (!this.providerInfoFor(source, providers)?.usable) return
    const current = this.activeSources()
    if (current.size === 1 && current.has(source)) {
      this.activateAllUsable(providers)
      return
    }

    this.activeSources.set(new Set<Source>([source]))
  }

  activateAllUsable(providers: readonly ProviderInfo[]): void {
    this.activeSources.set(new Set(providers.filter(provider => provider.usable).map(p => p.id)))
  }

  retainUsableSources(providers: readonly ProviderInfo[]): void {
    const usable = new Set(
      providers.filter(provider => provider.usable).map(provider => provider.id)
    )
    const next = new Set(Array.from(this.activeSources()).filter(source => usable.has(source)))
    this.activeSources.set(next)
  }

  clearSelection(): void {
    this.selectedEntry.set(null)
    this.selectedEntries.set(null)
  }

  private providerInfoFor(
    source: Source,
    providers: readonly ProviderInfo[]
  ): ProviderInfo | undefined {
    return providers.find(provider => provider.id === source)
  }

  private mondayOf(dateKey: string): string {
    const weekDay = dateOfKey(dateKey).getDay()
    return addDaysToKey(dateKey, -(weekDay === 0 ? 6 : weekDay - 1))
  }
}
