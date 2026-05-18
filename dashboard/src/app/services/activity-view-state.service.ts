import { Injectable, computed, signal } from '@angular/core'
import type { ProviderInfo, Source } from '../generated/activity'
import type { EntryUiAdapter } from '../providers/types'
import type { ViewMode } from './types'

@Injectable({ providedIn: 'root' })
export class ActivityViewStateService {
  readonly dateFrom = signal('')
  readonly dateTo = signal('')
  readonly searchQuery = signal('')
  readonly activeSources = signal(new Set<Source>())
  readonly currentView = signal<ViewMode>('list')
  readonly selectedDay = signal<string>(this.todayStr())
  readonly selectedEntry = signal<EntryUiAdapter | null>(null)
  readonly selectedEntries = signal<EntryUiAdapter[] | null>(null)
  readonly hourHeight = signal(64)

  readonly weekDays = computed<string[]>(() => {
    const start = this.mondayOf(this.selectedDay())
    return Array.from({ length: 7 }, (_, i) => {
      const day = new Date(start + 'T12:00:00')
      day.setDate(day.getDate() + i)
      return day.toISOString().slice(0, 10)
    })
  })

  readonly workWeekDays = computed<string[]>(() => this.weekDays().slice(0, 5))

  navigateDay(delta: number): void {
    const day = new Date(this.selectedDay() + 'T12:00:00')
    day.setDate(day.getDate() + delta)
    this.selectedDay.set(day.toISOString().slice(0, 10))
  }

  navigateWeek(delta: number): void {
    this.navigateDay(delta * 7)
  }

  goToToday(): void {
    this.selectedDay.set(this.todayStr())
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

  private todayStr(): string {
    return new Date().toISOString().slice(0, 10)
  }

  private mondayOf(dateStr: string): string {
    const day = new Date(dateStr + 'T12:00:00')
    const weekDay = day.getDay()
    day.setDate(day.getDate() - (weekDay === 0 ? 6 : weekDay - 1))
    return day.toISOString().slice(0, 10)
  }
}
