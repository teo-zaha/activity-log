import { Component, ElementRef, afterNextRender, computed, inject, viewChild } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { mediaQuery } from '@signality/core/browser/media-query'
import { IconButtonComponent } from '../button/icon-button.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityService } from '../../services/activity.service'
import type { EntryUiAdapter } from '../../providers/types'
import { downQuery } from '../../utils/breakpoints'
import { injectMinuteClock } from '../../utils/clock'
import { dateOfKey, dayKeyOf, keysWithin, type DayKey } from '../../utils/local-day'
import {
  NO_MEETING_TIME,
  meetingTimeByDay,
  meetingTimeLabel,
  meetingTimeParts,
} from '../../utils/meeting-time'
import {
  calendarEntryKey,
  calendarEventClass,
  calendarEventLabel,
  layoutEventsWithOverflow,
  nowMarker,
} from '../calendar-layout'
import { injectCalendarZoom } from '../calendar-zoom'

export const HOUR_START = 0
export const HOUR_END = 23

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

interface PositionedDayEvent {
  readonly kind: 'event'
  readonly key: string
  readonly entry: EntryUiAdapter
  readonly top: number
  readonly height: number
  readonly style: EventStyleVars
}

interface PositionedDayOverflow {
  readonly kind: 'overflow'
  readonly key: string
  readonly entries: EntryUiAdapter[]
  readonly top: number
  readonly height: number
  readonly style: EventStyleVars
}

interface PositionedAllDayEntry {
  readonly key: string
  readonly entry: EntryUiAdapter
  readonly top: number
  readonly height: number
  readonly left: string
  readonly width: string
  readonly right: string
}

@Component({
  selector: 'app-day-view',
  imports: [IconButtonComponent, ButtonComponent, MatIconModule],
  templateUrl: './day-view.component.html',
  styleUrl: './day-view.component.scss',
})
export class DayViewComponent {
  protected readonly activity = inject(ActivityService)
  private readonly isPhone = mediaQuery(downQuery('phone'))
  private readonly now = injectMinuteClock()
  private readonly today = computed(() => dayKeyOf(this.now()))

  private readonly scrollEl = viewChild.required<ElementRef<HTMLElement>>('scrollEl')
  private readonly gridEl = viewChild.required<ElementRef<HTMLElement>>('gridEl')
  protected readonly zoom = injectCalendarZoom(this.scrollEl, this.gridEl)

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

  protected formatDayHeader(dateStr: DayKey): string {
    const compact = this.isPhone()
    return dateOfKey(dateStr).toLocaleDateString('en-GB', {
      weekday: compact ? 'short' : 'long',
      day: 'numeric',
      month: compact ? 'short' : 'long',
      year: 'numeric',
    })
  }

  protected isToday(dateStr: string): boolean {
    return dateStr === this.today()
  }

  /** The current-time line, while the shown day is today. */
  protected readonly nowLine = computed(() =>
    this.activity.selectedDay() === this.today()
      ? nowMarker(this.now(), this.activity.hourHeight())
      : null
  )

  /** Only for a collected day: outside the range its meetings were not read. */
  private readonly meetingTime = computed(() => {
    const days = keysWithin(
      [this.activity.selectedDay()],
      this.activity.dateFrom(),
      this.activity.dateTo()
    )
    return meetingTimeByDay(this.activity.calendarEntries(), days).get(days[0]) ?? NO_MEETING_TIME
  })

  protected readonly meetingTimeParts = computed(() => meetingTimeParts(this.meetingTime()))
  protected readonly meetingTimeLabel = computed(() => meetingTimeLabel(this.meetingTime()))

  protected readonly eventClass = calendarEventClass

  protected getTop(ts: string): number {
    const d = new Date(ts)
    const h = this.activity.hourHeight()
    return Math.max(0, (d.getHours() - HOUR_START) * h + (d.getMinutes() / 60) * h)
  }

  protected getHeight(entry: EntryUiAdapter): number {
    const durationMinutes = entry.calendarDurationMinutes
    if (durationMinutes != null) {
      return Math.max(22, (durationMinutes / 60) * this.activity.hourHeight())
    }
    return 22
  }

  /** A meeting that runs past midnight; one that ends at midnight stays a block. */
  private isMultiDayEvent(e: EntryUiAdapter): boolean {
    return (e.source === 'outlook' || e.source === 'calendar') && e.lastDayKey > e.dayKey
  }

  readonly multiDayEntries = computed<EntryUiAdapter[]>(() => {
    const day = this.activity.selectedDay()
    return this.activity
      .calendarEntries()
      .filter(e => this.isMultiDayEvent(e) && e.dayKey <= day && e.lastDayKey >= day)
  })

  protected readonly allDayEntries = computed<PositionedAllDayEntry[]>(() =>
    this.multiDayEntries().map((entry, index) => ({
      key: calendarEntryKey(entry),
      entry,
      left: `${ALLDAY_BAR_SIDE_GAP}px`,
      right: `${ALLDAY_BAR_SIDE_GAP}px`,
      width: `calc(100% - ${ALLDAY_BAR_SIDE_GAP * 2}px)`,
      top: index * (ALLDAY_BAR_HEIGHT + ALLDAY_BAR_GAP) + ALLDAY_BAR_GAP,
      height: ALLDAY_BAR_HEIGHT,
    }))
  )

  protected allDayRowHeight(): number {
    const count = this.multiDayEntries().length
    return count === 0 ? 0 : count * (ALLDAY_BAR_HEIGHT + ALLDAY_BAR_GAP) + ALLDAY_BAR_GAP * 2
  }

  protected readonly dayLayout = computed<(PositionedDayEvent | PositionedDayOverflow)[]>(() => {
    const layout = layoutEventsWithOverflow(
      this.activity.dayEntries().filter(e => !this.isMultiDayEvent(e)),
      entry => this.getTop(entry.timestamp),
      entry => this.getHeight(entry),
      22,
      5
    )

    return layout.map(item => {
      const style = this.eventStyle(item.col, item.cols)
      if (item.kind === 'overflow') {
        return {
          kind: 'overflow',
          key: `overflow:${item.entries.map(calendarEntryKey).join('|')}`,
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
  })

  private eventStyle(col: number, cols: number): EventStyleVars {
    const pct = 100 / cols
    return {
      left: `calc(${col * pct}% + ${EVENT_GAP}px)`,
      width: `calc(${pct}% - ${EVENT_GAP * 2}px)`,
      right: 'unset',
    }
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
}
