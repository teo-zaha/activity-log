import type { VirtualItem } from '@tanstack/angular-virtual'
import type { DayKey } from '../../utils/local-day'
import type { TimelineDay, TimelineRow } from './timeline-rows'

type TimelineItem =
  | { readonly kind: 'header'; readonly key: DayKey; readonly dayIndex: number }
  | { readonly kind: 'gap'; readonly key: string; readonly dayIndex: number }
  | {
      readonly kind: 'row'
      readonly key: TimelineRow['entry']['trackKey']
      readonly dayIndex: number
      readonly row: TimelineRow
    }

interface DayItems {
  readonly day: TimelineDay
  readonly header: number
  readonly gap: number
}

export interface TimelineItems {
  readonly items: readonly TimelineItem[]
  readonly days: readonly DayItems[]
  readonly indexes: ReadonlyMap<string, number>
}

/** The measured header and rows of each day, followed by its fixed bottom padding. */
export function timelineItems(days: readonly TimelineDay[]): TimelineItems {
  const items: TimelineItem[] = []
  const indexes = new Map<string, number>()
  const dayItems = days.map((day, dayIndex) => {
    const header = items.length
    indexes.set(day.date, header)
    items.push({ kind: 'header', key: day.date, dayIndex })
    for (const row of day.rows) {
      const key = row.entry.trackKey
      indexes.set(key, items.length)
      items.push({ kind: 'row', key, dayIndex, row })
    }
    const gap = items.length
    items.push({ kind: 'gap', key: `gap/${day.date}`, dayIndex })
    return { day, header, gap }
  })
  return { items, days: dayItems, indexes }
}

interface DrawnRow {
  readonly row: TimelineRow
  readonly index: number
  readonly top: number
  readonly position: number
}

export interface DrawnDay {
  readonly day: TimelineDay
  readonly index: number
  readonly header: number
  readonly gap: number
  readonly height: number
  readonly rowsHeight: number
  readonly rows: DrawnRow[]
}

/**
 * The DOM still has one article per day. Its full measured or estimated height is the sticky
 * header's containing block, even when its rows are drawn in disjoint windows.
 */
export function drawnDays(
  layout: TimelineItems,
  measurements: readonly VirtualItem[],
  window: readonly VirtualItem[],
  listTop: number
): DrawnDay[] {
  const drawn: DrawnDay[] = []
  let end = listTop
  for (const item of window) {
    const model = layout.items[item.index]
    if (!model) continue
    const day = layout.days[model.dayIndex]!
    const header = measurements[day.header]
    const gap = measurements[day.gap]
    if (!header || !gap) continue
    let section = drawn.at(-1)
    if (section?.index !== model.dayIndex) {
      section = {
        day: day.day,
        index: model.dayIndex,
        header: day.header,
        gap: header.start - end,
        height: gap.end - header.start,
        rowsHeight: gap.start - header.end,
        rows: [],
      }
      drawn.push(section)
      end = gap.end
    }
    if (model.kind === 'row') {
      section.rows.push({
        row: model.row,
        index: item.index,
        top: item.start - header.end,
        position: item.index - day.header,
      })
    }
  }
  return drawn
}
