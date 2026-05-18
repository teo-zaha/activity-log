import {
  AfterViewInit,
  ChangeDetectionStrategy,
  Component,
  ElementRef,
  ViewChild,
  computed,
  inject,
} from '@angular/core'
import { ActivityService } from '../../services/activity.service'
import type { EntryUiAdapter } from '../../providers/types'
import { calendarEntryKey, layoutEventsWithOverflow } from '../calendar-layout'

const HOUR_START = 0
const HOUR_END = 23
const DEFAULT_SCROLL_HOUR = 6.5 // 06:30
const ALLDAY_BAR_HEIGHT = 26
const ALLDAY_BAR_GAP = 2
const ALLDAY_BAR_SIDE_GAP = 3
const EVENT_GAP = 4

interface EventStyleVars {
  readonly left: string
  readonly width: string
  readonly right: string
}

interface MultiDayBar {
  entry: EntryUiAdapter
  startCol: number
  spanCols: number
  row: number
}

interface PositionedMultiDayBar extends MultiDayBar {
  readonly key: string
  readonly left: string
  readonly width: string
  readonly top: number
  readonly height: number
}

interface PositionedWeekEvent {
  readonly kind: 'event'
  readonly key: string
  readonly entry: EntryUiAdapter
  readonly top: number
  readonly height: number
  readonly style: EventStyleVars
}

interface PositionedWeekOverflow {
  readonly kind: 'overflow'
  readonly key: string
  readonly entries: EntryUiAdapter[]
  readonly top: number
  readonly height: number
  readonly style: EventStyleVars
}

@Component({
  selector: 'app-week-view',
  templateUrl: './week-view.component.html',
  styleUrl: './week-view.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class WeekViewComponent implements AfterViewInit {
  protected readonly activity = inject(ActivityService)

  @ViewChild('scrollEl') private scrollEl!: ElementRef<HTMLElement>

  protected readonly hours = Array.from(
    { length: HOUR_END - HOUR_START + 1 },
    (_, i) => HOUR_START + i
  )

  protected gridHeight(): number {
    return (HOUR_END - HOUR_START + 1) * this.activity.hourHeight()
  }

  ngAfterViewInit(): void {
    this.scrollEl.nativeElement.scrollTop = DEFAULT_SCROLL_HOUR * this.activity.hourHeight()
  }

  protected formatWeekRange(days: string[]): string {
    if (!days.length) return ''
    const s = new Date(days[0] + 'T12:00:00')
    const e = new Date(days[days.length - 1] + 'T12:00:00')
    const month =
      s.getMonth() === e.getMonth()
        ? s.toLocaleDateString('en-GB', { month: 'long', year: 'numeric' })
        : `${s.toLocaleDateString('en-GB', { month: 'short' })} – ${e.toLocaleDateString('en-GB', { month: 'short', year: 'numeric' })}`
    return month
  }

  protected formatDayCol(dateStr: string): { weekday: string; date: string } {
    const d = new Date(dateStr + 'T12:00:00')
    return {
      weekday: d.toLocaleDateString('en-GB', { weekday: 'short' }),
      date: String(d.getDate()),
    }
  }

  protected isToday(dateStr: string): boolean {
    return dateStr === new Date().toISOString().slice(0, 10)
  }

  protected isSelectedDay(dateStr: string): boolean {
    return dateStr === this.activity.selectedDay()
  }

  protected get days(): string[] {
    return this.activity.currentView() === 'workweek'
      ? this.activity.workWeekDays()
      : this.activity.weekDays()
  }

  // ── Multi-day event support ───────────────────────────────────────────────

  private isMultiDayEvent(e: EntryUiAdapter): boolean {
    if (e.source !== 'outlook') return false
    const durationMinutes = e.calendarDurationMinutes
    if (durationMinutes == null) return false
    const endMs = new Date(e.timestamp).getTime() + durationMinutes * 60000
    return new Date(endMs).toISOString().slice(0, 10) > e.timestamp.slice(0, 10)
  }

  readonly multiDayLayout = computed<MultiDayBar[]>(() => {
    const days = this.days
    if (!days.length) return []
    const weekStart = days[0]
    const weekEnd = days[days.length - 1]

    // Collect multi-day events overlapping this week
    const events = this.activity.filteredEntries().filter(e => {
      if (!this.isMultiDayEvent(e)) return false
      const endMs = new Date(e.timestamp).getTime() + (e.calendarDurationMinutes ?? 0) * 60000
      const endDay = new Date(endMs).toISOString().slice(0, 10)
      const startDay = e.timestamp.slice(0, 10)
      return startDay <= weekEnd && endDay > weekStart
    })

    // Determine column span for each event (clipped to visible week)
    const bars: Omit<MultiDayBar, 'row'>[] = events.map(ev => {
      const endMs = new Date(ev.timestamp).getTime() + (ev.calendarDurationMinutes ?? 0) * 60000
      const endDay = new Date(endMs).toISOString().slice(0, 10)
      const startDay = ev.timestamp.slice(0, 10)
      const clippedStart = startDay < weekStart ? weekStart : startDay
      const clippedEnd =
        endDay > weekEnd
          ? weekEnd
          : new Date(new Date(endMs).getTime() - 1).toISOString().slice(0, 10)
      const startCol = days.indexOf(clippedStart)
      const endCol = days.indexOf(clippedEnd)
      return {
        entry: ev,
        startCol: Math.max(0, startCol),
        spanCols: Math.max(1, endCol - Math.max(0, startCol) + 1),
      }
    })

    // Greedy row packing
    const rowEnds: number[] = [] // tracks the last endCol+1 used per row
    return bars.map(bar => {
      let row = rowEnds.findIndex(end => end <= bar.startCol)
      if (row === -1) row = rowEnds.length
      rowEnds[row] = bar.startCol + bar.spanCols
      return { ...bar, row }
    })
  })

  protected readonly positionedMultiDayLayout = computed<PositionedMultiDayBar[]>(() =>
    this.multiDayLayout().map(bar => {
      const total = this.days.length
      return {
        ...bar,
        key: calendarEntryKey(bar.entry),
        left: `calc(${(bar.startCol / total) * 100}% + ${ALLDAY_BAR_SIDE_GAP}px)`,
        width: `calc(${(bar.spanCols / total) * 100}% - ${ALLDAY_BAR_SIDE_GAP * 2}px)`,
        top: bar.row * (ALLDAY_BAR_HEIGHT + ALLDAY_BAR_GAP) + ALLDAY_BAR_GAP,
        height: ALLDAY_BAR_HEIGHT,
      }
    })
  )

  protected allDayRowHeight(): number {
    const maxRow = this.multiDayLayout().reduce((m, b) => Math.max(m, b.row), -1)
    return maxRow < 0 ? 0 : (maxRow + 1) * (ALLDAY_BAR_HEIGHT + ALLDAY_BAR_GAP) + ALLDAY_BAR_GAP * 2
  }

  protected dayHasMultiDayEvent(dateStr: string): boolean {
    return this.multiDayLayout().some(bar => {
      const start = this.days[bar.startCol]
      const end = this.days[bar.startCol + bar.spanCols - 1]
      return dateStr >= start && dateStr <= end
    })
  }

  // ── Per-day layout ────────────────────────────────────────────────────────

  protected getEntriesForDay(dateStr: string): EntryUiAdapter[] {
    return this.activity.weekEntriesByDay().get(dateStr) ?? []
  }

  protected readonly layoutsByDay = computed<
    Map<string, (PositionedWeekEvent | PositionedWeekOverflow)[]>
  >(() => {
    const maxVisible = this.activity.currentView() === 'workweek' ? 4 : 3
    const map = new Map<string, (PositionedWeekEvent | PositionedWeekOverflow)[]>()

    for (const day of this.days) {
      const entries = this.getEntriesForDay(day).filter(e => !this.isMultiDayEvent(e))
      const layout = layoutEventsWithOverflow(
        entries,
        (entry: EntryUiAdapter) => this.getTop(entry.timestamp),
        (entry: EntryUiAdapter) => this.getHeight(entry),
        22,
        maxVisible
      )

      map.set(
        day,
        layout.map(item => {
          const style = this.eventStyle(item.col, item.cols)
          if (item.kind === 'overflow') {
            return {
              kind: 'overflow',
              key: `overflow:${day}:${item.entries.map(calendarEntryKey).join('|')}`,
              entries: item.entries,
              top: item.top,
              height: item.height,
              style,
            }
          }

          return {
            kind: 'event',
            key: calendarEntryKey(item.entry),
            entry: item.entry,
            top: this.getTop(item.entry.timestamp),
            height: this.getHeight(item.entry),
            style,
          }
        })
      )
    }

    return map
  })

  protected getLayoutForDay(dateStr: string): (PositionedWeekEvent | PositionedWeekOverflow)[] {
    return this.layoutsByDay().get(dateStr) ?? []
  }

  private eventStyle(col: number, cols: number): EventStyleVars {
    const pct = 100 / cols
    return {
      left: `calc(${col * pct}% + ${EVENT_GAP}px)`,
      width: `calc(${pct}% - ${EVENT_GAP * 2}px)`,
      right: 'unset',
    }
  }

  protected getTop(ts: string): number {
    const d = new Date(ts)
    const h = this.activity.hourHeight()
    return Math.max(0, (d.getHours() - HOUR_START) * h + (d.getMinutes() / 60) * h)
  }

  protected getHeight(entry: EntryUiAdapter): number {
    const durationMinutes = entry.calendarDurationMinutes
    if (durationMinutes != null) {
      return Math.max(20, (durationMinutes / 60) * this.activity.hourHeight())
    }
    return 20
  }

  protected zoomOut(): void {
    this.activity.hourHeight.update(h => Math.max(32, h - 16))
  }
  protected zoomIn(): void {
    this.activity.hourHeight.update(h => Math.min(192, h + 16))
  }

  protected selectDay(dateStr: string): void {
    this.activity.selectedDay.set(dateStr)
    this.activity.setView('day')
  }

  protected selectEntry(entry: EntryUiAdapter): void {
    this.activity.selectedEntries.set(null)
    this.activity.selectedEntry.set(entry)
  }

  protected selectOverflow(entries: EntryUiAdapter[]): void {
    this.activity.selectedEntry.set(null)
    this.activity.selectedEntries.set(entries)
  }

  protected formatHour(h: number): string {
    return `${String(h).padStart(2, '0')}:00`
  }

  protected eventAriaLabel(entry: EntryUiAdapter): string {
    return `Open event: ${entry.title}`
  }

  protected overflowAriaLabel(entries: readonly EntryUiAdapter[]): string {
    return `Open ${entries.length} additional events`
  }

  protected dayAriaLabel(day: string): string {
    return `Open day view for ${this.formatDayCol(day).weekday} ${this.formatDayCol(day).date}`
  }
}
