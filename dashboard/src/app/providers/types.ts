import type { ActivityEntry, ProviderInfo, Source } from '../generated/activity'
import { clockTime, dayKeyOf } from '../utils/local-day'
import { type ConnectionKey, entryConnectionKeys } from './connection'

export type EntryBySource<S extends Source> = Extract<ActivityEntry, { source: S }>

export interface ProviderDetailField {
  label: string
  value: string
}

export interface EntryBadgeVm {
  source: Source
  label: string
  icon: string
  badgeClass: string
}

export interface EntryDetailSection {
  source: Source
  title: string
  icon: string
  badgeClass: string
  fields: readonly ProviderDetailField[]
}

interface EntryUiAdapterInit {
  title: string
  meta: string
  entryIcon: string
  searchTerms: readonly string[]
  calendarDurationMinutes: number | null
  detailFields: readonly ProviderDetailField[]
  badge: EntryBadgeVm
  badges: readonly EntryBadgeVm[]
  detailSections: readonly EntryDetailSection[]
  styleKey: string
}

export abstract class ProviderUiAdapterDefinition<
  S extends Source = Source,
  TEntry extends EntryBySource<S> = EntryBySource<S>,
  TAdapter extends EntryUiAdapter<TEntry> = EntryUiAdapter<TEntry>,
> {
  abstract readonly source: S
  abstract readonly label: string
  abstract readonly providerIcon: string

  getStatusTooltip(info: ProviderInfo): string | null {
    return info.reason ?? null
  }

  abstract create(entry: TEntry): TAdapter
}

export abstract class EntryUiAdapter<TEntry extends ActivityEntry = ActivityEntry> {
  readonly source: TEntry['source']
  readonly timestamp: string
  /** The local day the entry falls on, `YYYY-MM-DD`. */
  readonly dayKey: string
  /** The local time, `14:05`; worked out once, since lists show it for every entry. */
  readonly timeLabel: string
  readonly title: string
  readonly meta: string
  readonly entryIcon: string
  readonly searchTerms: readonly string[]
  readonly calendarDurationMinutes: number | null
  readonly detailFields: readonly ProviderDetailField[]
  readonly badge: EntryBadgeVm
  readonly badges: readonly EntryBadgeVm[]
  readonly detailSections: readonly EntryDetailSection[]
  readonly styleKey: string
  /** The connections the entry came from, see `entryConnectionKeys`. */
  readonly connections: readonly ConnectionKey[]
  /** Identifies the entry across collections, so lists keep the rows of entries that stay. */
  readonly trackKey: string

  protected constructor(
    readonly provider: ProviderUiAdapterDefinition<Source, TEntry>,
    readonly entry: TEntry,
    init: EntryUiAdapterInit
  ) {
    this.source = entry.source
    this.timestamp = entry.timestamp
    this.dayKey = dayKeyOf(entry.timestamp)
    this.timeLabel = clockTime(entry.timestamp)
    this.title = init.title
    this.meta = init.meta
    this.entryIcon = init.entryIcon
    this.searchTerms = init.searchTerms
    this.calendarDurationMinutes = init.calendarDurationMinutes
    this.detailFields = init.detailFields
    this.badge = init.badge
    this.badges = init.badges
    this.detailSections = init.detailSections
    this.styleKey = init.styleKey
    this.connections = entryConnectionKeys(entry)
    this.trackKey = [entry.source, entry.connection, entry.timestamp, init.title, init.meta].join(
      '\u001f'
    )
  }

  abstract markdownSummary(): string
  abstract markdownMeta(timeLabel: string): string
}
