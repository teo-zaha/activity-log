import { Service, computed, inject, type Signal } from '@angular/core'
import type { DayGroup } from '../models/day-group'
import type { ConnectionKey } from '../providers/connection'
import type { EntryUiAdapter } from '../providers/types'
import type { DayKey } from '../utils/local-day'
import { ActivityDataService } from './activity-data.service'
import { ActivityViewStateService, type DayRange } from './activity-view-state.service'

/** How the collection on screen covers this tab's own range (the URL's `from`/`to`). */
export type RangeCoverage =
  /** No range of its own, or nothing collected yet: nothing to compare. */
  | { readonly kind: 'unknown' }
  | { readonly kind: 'covered' }
  /** Another collection is on screen (another client, or before this tab collected its own). */
  | {
      readonly kind: 'other'
      readonly collected: DayRange
      readonly own: DayRange
      /** Some of this tab's days are in the collection: their entries show, the rest are missing. */
      readonly overlaps: boolean
    }

/**
 * What the views show of the collection: the entries of the sources shown, in this tab's range,
 * the open event or group, and how the collection on screen covers this tab's range.
 */
@Service()
export class ActivitySelection {
  private readonly data = inject(ActivityDataService)
  private readonly viewState = inject(ActivityViewStateService)

  /** Every entry by its `trackKey`, for the details the URL names. */
  private readonly entryByKey = computed(
    () => new Map(this.data.allEntryAdapters().map(entry => [entry.trackKey, entry]))
  )

  /** The open event, once its entry is loaded. */
  readonly selectedEntry: Signal<EntryUiAdapter | null> = computed(() => {
    const key = this.viewState.openEventKey()
    return key === null ? null : (this.entryByKey().get(key) ?? null)
  })

  /** The listed group's loaded events; null without a group. */
  readonly selectedEntries: Signal<readonly EntryUiAdapter[] | null> = computed(() => {
    const keys = this.viewState.openGroupKeys()
    if (keys.length === 0) return null
    const byKey = this.entryByKey()
    return keys.flatMap(key => byKey.get(key) ?? [])
  })

  /** Entries of the connections the source filter shows. */
  private readonly shownEntries = computed(() => {
    const active = this.viewState.activeConnections()
    return this.data
      .allEntryAdapters()
      .filter(entry => entry.connections.some(key => active.has(key)))
  })

  /** Entries of the connections the source filter shows, in this tab's range; Timeline filter applied. */
  readonly filteredEntries: Signal<readonly EntryUiAdapter[]> = computed(() => {
    const from = this.viewState.dateFrom()
    const to = this.viewState.dateTo()
    const query =
      this.viewState.currentView() === 'list' ? this.viewState.searchQuery().toLowerCase() : ''

    return this.shownEntries().filter(entry => {
      const day = entry.dayKey
      if (from && day < from) return false
      if (to && day > to) return false
      if (query && !entry.searchText.includes(query)) return false
      return true
    })
  })

  /**
   * What the calendar views draw: like `filteredEntries` without the filter, but a meeting that
   * starts before the range and runs into it stays, for its blocks and meeting time on the days
   * it covers.
   */
  readonly calendarEntries: Signal<readonly EntryUiAdapter[]> = computed(() => {
    const from = this.viewState.dateFrom()
    const to = this.viewState.dateTo()
    return this.shownEntries().filter(
      entry => (!from || entry.lastDayKey >= from) && (!to || entry.dayKey <= to)
    )
  })

  /** `filteredEntries` by local day, newest day first. */
  readonly groupedByDay: Signal<readonly DayGroup<EntryUiAdapter>[]> = computed(() => {
    const byDay = new Map<DayKey, EntryUiAdapter[]>()
    for (const entry of this.filteredEntries()) {
      const day = entry.dayKey
      const entries = byDay.get(day)
      if (entries) entries.push(entry)
      else byDay.set(day, [entry])
    }
    return Array.from(byDay.entries())
      .sort(([a], [b]) => b.localeCompare(a))
      .map(([date, entries]) => ({ date, entries }))
  })

  /** What Export writes for the view on screen: the Timeline's list or what a calendar view draws. */
  readonly exportedEntries: Signal<readonly EntryUiAdapter[]> = computed(() =>
    this.viewState.currentView() === 'list' ? this.filteredEntries() : this.calendarEntries()
  )

  /**
   * How the collection on screen covers this tab's range. Without a range of its own the tab
   * shows the collected one (`ActivityViewStateService.dateFrom`), which then counts as covered.
   */
  readonly rangeCoverage: Signal<RangeCoverage> = computed(() => {
    const collected = this.viewState.collectedRange()
    const [from, to] = [this.viewState.dateFrom(), this.viewState.dateTo()]
    if (collected === null || from === null || to === null) return { kind: 'unknown' }
    if (from >= collected.from && to <= collected.to) return { kind: 'covered' }
    return {
      kind: 'other',
      collected,
      own: { from, to },
      overlaps: from <= collected.to && to >= collected.from,
    }
  })

  /** Shows or hides a connection's entries, against the sources on screen. */
  toggleConnection(key: ConnectionKey): void {
    this.viewState.toggleConnection(key, this.data.providerInfo())
  }

  /** Shows only a connection's entries, or all again, against the sources on screen. */
  isolateConnection(key: ConnectionKey): void {
    this.viewState.isolateConnection(key, this.data.providerInfo())
  }
}
