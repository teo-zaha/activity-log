import type { ActivityEntry, ProviderInfo, Source } from '../generated/activity'

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
  sources: readonly Source[]
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
  readonly sources: readonly Source[]

  protected constructor(
    readonly provider: ProviderUiAdapterDefinition<Source, TEntry>,
    readonly entry: TEntry,
    init: EntryUiAdapterInit
  ) {
    this.source = entry.source
    this.timestamp = entry.timestamp
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
    this.sources = init.sources
  }

  abstract markdownSummary(): string
  abstract markdownMeta(timeLabel: string): string
}
