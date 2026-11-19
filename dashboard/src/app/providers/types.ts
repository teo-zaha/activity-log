import type {
  ActivityEntry,
  BusyStatus,
  MeetingWeight,
  DescriptionBlock,
  ProviderInfo,
  Source,
} from '../generated/activity'
import { clockTime, dayKeyOf } from '../utils/local-day'
import { type ConnectionKey, type EntryKey, entryConnectionKeys } from './connection'

export type EntryBySource<S extends Source> = Extract<ActivityEntry, { source: S }>

/** Identifies an entry across collections: the backend's id is unique within its connection. */
export function entryTrackKey(entry: ActivityEntry): EntryKey {
  return `${entry.source}/${entry.connection}/${entry.id}`
}

export interface ProviderDetailField {
  label: string
  value: string
}

export interface EntryBadgeVm {
  source: Source
  label: string
  icon: string
  /** A second source sharing the badge in its own colours, such as `LAB` after `GIT`. */
  split?: EntryBadgeSplit
}

export interface EntryBadgeSplit {
  source: Source
  label: string
}

export interface EntryDetailSection {
  source: Source
  title: string
  icon: string
  fields: readonly ProviderDetailField[]
}

interface EntryUiAdapterInit {
  title: string
  meta: string
  entryIcon: string
  searchTerms: readonly string[]
  calendarDurationMinutes: number | null
  /** Meetings only; null when the source does not say. */
  busyStatus?: BusyStatus | null
  /** Meetings only: how the backend counts it towards meeting time. */
  meetingWeight?: MeetingWeight
  /** Meetings only: the description as plain text, null when there is none. */
  description?: string | null
  /** Meetings only: the description as paragraphs, headings and lists, from the backend. */
  descriptionBlocks?: readonly DescriptionBlock[]
  /** Meetings only: the https link that joins the online meeting. */
  joinUrl?: string | null
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
  /** The local day of the entry's last minute: later than `dayKey` for a meeting past midnight. */
  readonly lastDayKey: string
  /** The local time, `14:05`; worked out once, since lists show it for every entry. */
  readonly timeLabel: string
  readonly title: string
  readonly meta: string
  readonly entryIcon: string
  readonly searchTerms: readonly string[]
  /** `searchTerms` lowercased into one string, joined once: descriptions make it long. */
  readonly searchText: string
  readonly calendarDurationMinutes: number | null
  /** How a meeting shows in free/busy time; null for other entries and when unknown. */
  readonly busyStatus: BusyStatus | null
  /** How a meeting counts towards meeting time; null for entries that are no meetings. */
  readonly meetingWeight: MeetingWeight | null
  /** A meeting's description as plain text; null for other entries and when it has none. */
  readonly description: string | null
  /** `description` as paragraphs, headings and lists; empty when there is none. */
  readonly descriptionBlocks: readonly DescriptionBlock[]
  /** The link that joins an online meeting; null for other entries and when there is none. */
  readonly joinUrl: string | null
  readonly detailFields: readonly ProviderDetailField[]
  readonly badge: EntryBadgeVm
  readonly badges: readonly EntryBadgeVm[]
  readonly detailSections: readonly EntryDetailSection[]
  readonly styleKey: string
  /** The connections the entry came from, see `entryConnectionKeys`. */
  readonly connections: readonly ConnectionKey[]
  /**
   * Identifies the entry across collections, so lists keep the rows of entries that stay: the
   * backend's id is unique within its connection.
   */
  readonly trackKey: EntryKey

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
    this.searchText = init.searchTerms.join(' ').toLowerCase()
    this.calendarDurationMinutes = init.calendarDurationMinutes
    const durationMinutes = init.calendarDurationMinutes ?? 0
    this.lastDayKey =
      durationMinutes > 0
        ? dayKeyOf(Date.parse(entry.timestamp) + durationMinutes * 60_000 - 1)
        : this.dayKey
    this.busyStatus = init.busyStatus ?? null
    this.meetingWeight = init.meetingWeight ?? null
    this.description = init.description ?? null
    this.descriptionBlocks = init.descriptionBlocks ?? []
    this.joinUrl = init.joinUrl ?? null
    this.detailFields = init.detailFields
    this.badge = init.badge
    this.badges = init.badges
    this.detailSections = init.detailSections
    this.styleKey = init.styleKey
    this.connections = entryConnectionKeys(entry)
    this.trackKey = entryTrackKey(entry)
  }
}
