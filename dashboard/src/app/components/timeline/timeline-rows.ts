import type { DayGroup } from '../../models/day-group'
import type { EntryUiAdapter } from '../../providers/types'
import { busyMarkerOf } from '../../providers/utils'
import { longDayLabel, relativeDayLabel, type DayKey } from '../../utils/local-day'
import { sourceLogo, type BrandIcon } from '../provider-icon/provider-icon.component'

/** One entry of the list, with what its row shows worked out once per entry. */
export interface TimelineRow {
  readonly entry: EntryUiAdapter
  readonly ariaLabel: string
  /** The source's logo for the badge; null when it shows `entry.badge.icon` instead. */
  readonly logo: BrandIcon | null
  /** Tentative and free meetings get a tag. */
  readonly busy: { readonly label: string; readonly className: string } | null
}

export interface TimelineDay {
  readonly date: DayKey
  /** `Monday, 16 November 2026` */
  readonly label: string
  /** `Today` or `Yesterday`, else null. */
  readonly relative: string | null
  /** `3 events` */
  readonly count: string
  readonly rows: readonly TimelineRow[]
}

const BUSY_TAGS = { tentative: 'Tentative', free: 'Free' } as const

export function timelineRow(entry: EntryUiAdapter): TimelineRow {
  const marker = busyMarkerOf(entry.busyStatus)
  const status = [marker, entry.joinUrl ? 'online meeting' : null].filter(Boolean).join(', ')
  const details = status ? ` (${status})` : ''
  return {
    entry,
    ariaLabel: `Open ${entry.badges.map(badge => badge.label).join(' and ')} event: ${entry.title}${details} at ${entry.timeLabel}`,
    logo: sourceLogo(entry.badge.source),
    busy: marker && { label: BUSY_TAGS[marker], className: `busy-tag is-${marker}` },
  }
}

function sameRows(a: readonly TimelineRow[], b: readonly TimelineRow[]): boolean {
  return a.length === b.length && a.every((row, index) => row === b[index])
}

function timelineDay(
  date: DayKey,
  relative: string | null,
  rows: readonly TimelineRow[]
): TimelineDay {
  const count = rows.length
  return {
    date,
    label: longDayLabel(date),
    relative,
    count: `${count} ${count === 1 ? 'event' : 'events'}`,
    rows,
  }
}

/**
 * The list, ready to draw, from one answer to the next: the template binds plain values, not
 * per-row function calls, and what did not change keeps its objects.
 */
export class TimelineRows {
  /** Rows by entry: an entry kept across answers and filters keeps its row's strings. */
  private readonly rows = new WeakMap<EntryUiAdapter, TimelineRow>()
  /**
   * The days last drawn. A day whose rows are the same objects as before stays the same object,
   * so its list is not compared row by row again when another day changes.
   */
  private drawn = new Map<DayKey, TimelineDay>()

  /** The days of `groups`, labelled relative to `today`. */
  days(groups: readonly DayGroup<EntryUiAdapter>[], today: DayKey): TimelineDay[] {
    const drawn = new Map<DayKey, TimelineDay>()
    const days = groups.map(group => {
      const rows = group.entries.map(entry => this.rowOf(entry))
      const previous = this.drawn.get(group.date)
      const relative = relativeDayLabel(group.date, today)
      const day =
        previous && previous.relative === relative && sameRows(previous.rows, rows)
          ? previous
          : timelineDay(group.date, relative, rows)
      drawn.set(group.date, day)
      return day
    })
    this.drawn = drawn
    return days
  }

  private rowOf(entry: EntryUiAdapter): TimelineRow {
    let row = this.rows.get(entry)
    if (!row) {
      row = timelineRow(entry)
      this.rows.set(entry, row)
    }
    return row
  }
}
