import { Component, ElementRef, afterNextRender, computed, inject, viewChild } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { IconButtonComponent } from '../button/icon-button.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityService } from '../../services/activity.service'
import type { EntryUiAdapter } from '../../providers/types'
import {
  calendarEntryKey,
  calendarEventClass,
  calendarEventLabel,
  layoutEventsWithOverflow,
  nowMarker,
} from '../calendar-layout'
import { injectMinuteClock } from '../../utils/clock'
import { dateOfKey, dayKeyOf, keysWithin, type DayKey } from '../../utils/local-day'
import {
  type MeetingTime,
  NO_MEETING_TIME,
  meetingTimeByDay,
  meetingTimeLabel,
  meetingTimeParts,
  sumMeetingTime,
} from '../../utils/meeting-time'
import { injectCalendarZoom } from '../calendar-zoom'
import { NavigationMotionService } from '../../navigation-motion'

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
  imports: [IconButtonComponent, ButtonComponent, MatIconModule],
  templateUrl: './week-view.component.html',
  styleUrl: './week-view.component.scss',
  // Cross-fades with the calendar view it replaces (`navigation-motion.ts`, `styles.scss`).
  host: {
    '(animate.enter)': "motion.enter($event, 'view')",
    '(animate.leave)': "motion.leave($event, 'view')",
  },
})
export class WeekViewComponent {
  protected readonly activity = inject(ActivityService)
  protected readonly motion = inject(NavigationMotionService)

  private readonly scrollEl = viewChild.required<ElementRef<HTMLElement>>('scrollEl')
  private readonly gridEl = viewChild.required<ElementRef<HTMLElement>>('gridEl')
  protected readonly zoom = injectCalendarZoom(this.scrollEl, this.gridEl)
  private readonly now = injectMinuteClock()
  private readonly today = computed(() => dayKeyOf(this.now()))

  protected readonly hours = Array.from(
    { length: HOUR_END - HOUR_START + 1 },
    (_, i) => HOUR_START + i
  )

  constructor() {
    afterNextRender({
      write: () => {
        this.scrollEl().nativeElement.scrollTop = DEFAULT_SCROLL_HOUR * this.activity.hourHeight()
      },
    })
  }

  protected gridHeight(): number {
    return (HOUR_END - HOUR_START + 1) * this.activity.hourHeight()
  }

  protected formatWeekRange(days: readonly DayKey[]): string {
    if (!days.length) return ''
    const s = dateOfKey(days[0])
    const e = dateOfKey(days[days.length - 1])
    const month =
      s.getMonth() === e.getMonth()
        ? s.toLocaleDateString('en-GB', { month: 'long', year: 'numeric' })
        : `${s.toLocaleDateString('en-GB', { month: 'short' })} – ${e.toLocaleDateString('en-GB', { month: 'short', year: 'numeric' })}`
    return month
  }

  protected formatDayCol(dateStr: DayKey): { weekday: string; date: string } {
    const d = dateOfKey(dateStr)
    return {
      weekday: d.toLocaleDateString('en-GB', { weekday: 'short' }),
      date: String(d.getDate()),
    }
  }

  protected isToday(dateStr: DayKey): boolean {
    return dateStr === this.today()
  }

  /** The current-time line, while today is one of the shown days. */
  protected readonly nowLine = computed(() =>
    this.days().includes(this.today()) ? nowMarker(this.now(), this.activity.hourHeight()) : null
  )

  /** Only for the collected days: a day outside the range has meetings that were not read. */
  private readonly meetingTimes = computed(() =>
    meetingTimeByDay(
      this.activity.calendarEntries(),
      keysWithin(this.days(), this.activity.dateFrom(), this.activity.dateTo())
    )
  )

  private readonly weekMeetingTime = computed(() => sumMeetingTime(this.meetingTimes().values()))

  protected readonly weekMeetingTimeParts = computed(() => meetingTimeParts(this.weekMeetingTime()))

  protected readonly weekMeetingTimeLabel = computed(() => meetingTimeLabel(this.weekMeetingTime()))

  private meetingTimeOf(day: DayKey): MeetingTime {
    return this.meetingTimes().get(day) ?? NO_MEETING_TIME
  }

  protected dayMeetingTimeParts(day: DayKey): ReturnType<typeof meetingTimeParts> {
    return meetingTimeParts(this.meetingTimeOf(day))
  }

  protected readonly eventClass = calendarEventClass

  protected isSelectedDay(dateStr: DayKey): boolean {
    return dateStr === this.activity.selectedDay()
  }

  protected readonly days = computed<DayKey[]>(() =>
    this.activity.currentView() === 'workweek'
      ? this.activity.workWeekDays()
      : this.activity.weekDays()
  )

  // ── Multi-day event support ───────────────────────────────────────────────

  /**
   * A meeting that runs past midnight; one that ends at midnight stays a block. Meetings are the
   * entries the backend weighs for meeting time, `none` included.
   */
  private isMultiDayEvent(e: EntryUiAdapter): boolean {
    return e.meetingWeight !== null && e.lastDayKey > e.dayKey
  }

  readonly multiDayLayout = computed<MultiDayBar[]>(() => {
    const days = this.days()
    if (!days.length) return []
    const weekStart = days[0]
    const weekEnd = days[days.length - 1]

    // Collect multi-day events overlapping this week
    const events = this.activity
      .calendarEntries()
      .filter(e => this.isMultiDayEvent(e) && e.dayKey <= weekEnd && e.lastDayKey >= weekStart)

    // Determine column span for each event (clipped to visible week)
    const bars: Omit<MultiDayBar, 'row'>[] = events.map(ev => {
      const startDay = ev.dayKey
      const clippedStart = startDay < weekStart ? weekStart : startDay
      const clippedEnd = ev.lastDayKey > weekEnd ? weekEnd : ev.lastDayKey
      const startCol = days.indexOf(clippedStart)
      const endCol = days.indexOf(clippedEnd)
      return {
        entry: ev,
        startCol: Math.max(0, startCol),
        spanCols: Math.max(1, endCol - Math.max(0, startCol) + 1),
      }
    })

    // Greedy row packing in order of the first column, longer bars first: that needs only as
    // many rows as bars overlap on one day (the log lists entries newest first).
    bars.sort((a, b) => a.startCol - b.startCol || b.spanCols - a.spanCols)
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
      const total = this.days().length
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

  protected dayHasMultiDayEvent(dateStr: DayKey): boolean {
    return this.multiDayLayout().some(bar => {
      const start = this.days()[bar.startCol]
      const end = this.days()[bar.startCol + bar.spanCols - 1]
      return dateStr >= start && dateStr <= end
    })
  }

  // ── Per-day layout ────────────────────────────────────────────────────────

  protected getEntriesForDay(dateStr: DayKey): EntryUiAdapter[] {
    return this.activity.weekEntriesByDay().get(dateStr) ?? []
  }

  protected readonly layoutsByDay = computed<
    Map<string, (PositionedWeekEvent | PositionedWeekOverflow)[]>
  >(() => {
    const maxVisible = this.activity.currentView() === 'workweek' ? 4 : 3
    const map = new Map<string, (PositionedWeekEvent | PositionedWeekOverflow)[]>()

    for (const day of this.days()) {
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

  protected readonly isEmpty = computed(
    () =>
      this.multiDayLayout().length === 0 &&
      Array.from(this.layoutsByDay().values()).every(layout => layout.length === 0)
  )

  protected getLayoutForDay(dateStr: DayKey): (PositionedWeekEvent | PositionedWeekOverflow)[] {
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

  protected selectDay(dateStr: DayKey): void {
    void this.activity.openDay(dateStr)
  }

  protected selectEntry(entry: EntryUiAdapter): void {
    void this.activity.openEvent(entry.trackKey)
  }

  protected selectOverflow(entries: EntryUiAdapter[]): void {
    void this.activity.openGroup(entries.map(entry => entry.trackKey))
  }

  protected formatHour(h: number): string {
    return `${String(h).padStart(2, '0')}:00`
  }

  protected eventAriaLabel(entry: EntryUiAdapter): string {
    return calendarEventLabel(entry)
  }

  protected overflowAriaLabel(entries: readonly EntryUiAdapter[]): string {
    return `Open ${entries.length} additional events`
  }

  protected dayAriaLabel(day: DayKey): string {
    const label = `Open day view for ${this.formatDayCol(day).weekday} ${this.formatDayCol(day).date}`
    const meetings = meetingTimeLabel(this.meetingTimeOf(day))
    return meetings ? `${label}, meetings ${meetings}` : label
  }
}
