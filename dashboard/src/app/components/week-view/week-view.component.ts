import { Component, ElementRef, afterNextRender, computed, inject, viewChild } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { IconButtonComponent } from '../button/icon-button.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityService } from '../../services/activity.service'
import type { EntryKey } from '../../providers/connection'
import type { EntryUiAdapter } from '../../providers/types'
import {
  DEFAULT_SCROLL_HOUR,
  gridHeight,
  layoutAllDay,
  layoutDays,
  nowMarker,
  planCalendar,
  type DayColumn,
} from '../calendar/calendar-grid'
import { CalendarAllDayRowComponent } from '../calendar/calendar-all-day-row/calendar-all-day-row.component'
import { CalendarDayColumnComponent } from '../calendar/calendar-day-column/calendar-day-column.component'
import { CalendarHoursComponent } from '../calendar/calendar-hours/calendar-hours.component'
import { CALENDAR_TIME_ZONE } from '../calendar/calendar-time-zone'
import { injectMinuteClock } from '../../utils/clock'
import { dateOfKey, keysWithin, wallClock, type DayKey } from '../../utils/local-day'
import {
  NO_MEETING_TIME,
  meetingTimeByDay,
  meetingTimeLabel,
  meetingTimeParts,
  sumMeetingTime,
  type MeetingTimeParts,
} from '../../utils/meeting-time'
import { injectCalendarZoom } from '../calendar-zoom'
import { NavigationMotionService } from '../../navigation-motion'

/** A day of the week as its header and its column show it. */
interface WeekDay {
  readonly day: DayKey
  readonly weekday: string
  readonly date: string
  readonly isToday: boolean
  readonly isSelected: boolean
  /** A meeting past midnight covers it. */
  readonly hasAllDay: boolean
  readonly meetingTime: MeetingTimeParts | null
  readonly label: string
}

@Component({
  selector: 'app-week-view',
  imports: [
    IconButtonComponent,
    ButtonComponent,
    MatIconModule,
    CalendarAllDayRowComponent,
    CalendarDayColumnComponent,
    CalendarHoursComponent,
  ],
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
  private readonly timeZone = inject(CALENDAR_TIME_ZONE)

  private readonly scrollEl = viewChild.required<ElementRef<HTMLElement>>('scrollEl')
  private readonly gridEl = viewChild.required<ElementRef<HTMLElement>>('gridEl')
  protected readonly zoom = injectCalendarZoom(this.scrollEl, this.gridEl)
  private readonly clock = injectMinuteClock()
  private readonly today = computed(() => wallClock(this.clock(), this.timeZone).day)

  constructor() {
    afterNextRender({
      write: () => {
        this.scrollEl().nativeElement.scrollTop = DEFAULT_SCROLL_HOUR * this.activity.hourHeight()
      },
    })
  }

  protected readonly days = computed<DayKey[]>(() =>
    this.activity.currentView() === 'workweek'
      ? this.activity.workWeekDays()
      : this.activity.weekDays()
  )

  protected readonly gridHeight = computed(() => gridHeight(this.activity.hourHeight()))

  /** `November 2026`, or `Oct – Nov 2026` for a week across two months. */
  protected readonly title = computed(() => {
    const days = this.days()
    const s = dateOfKey(days[0])
    const e = dateOfKey(days[days.length - 1])
    return s.getMonth() === e.getMonth()
      ? s.toLocaleDateString('en-GB', { month: 'long', year: 'numeric' })
      : `${s.toLocaleDateString('en-GB', { month: 'short' })} – ${e.toLocaleDateString('en-GB', { month: 'short', year: 'numeric' })}`
  })

  /** The current-time line, while today is one of the shown days. */
  protected readonly nowLine = computed(() => {
    const marker = nowMarker(this.clock(), this.activity.hourHeight(), this.timeZone)
    return this.days().includes(marker.day) ? marker : null
  })

  private readonly plan = computed(() =>
    planCalendar(this.days(), this.activity.calendarEntries(), this.timeZone)
  )

  protected readonly allDay = computed(() => layoutAllDay(this.plan()))

  protected readonly columns = computed<DayColumn<EntryUiAdapter>[]>(() =>
    layoutDays(
      this.plan(),
      this.activity.hourHeight(),
      this.activity.currentView() === 'workweek' ? 4 : 3
    )
  )

  protected readonly isEmpty = computed(
    () =>
      this.allDay().bars.length === 0 && this.columns().every(column => column.items.length === 0)
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

  protected readonly weekDays = computed<WeekDay[]>(() => {
    const [today, selected, covered] = [
      this.today(),
      this.activity.selectedDay(),
      this.allDay().days,
    ]
    return this.days().map(day => {
      const date = dateOfKey(day)
      const weekday = date.toLocaleDateString('en-GB', { weekday: 'short' })
      const meetingTime = this.meetingTimes().get(day) ?? NO_MEETING_TIME
      const meetings = meetingTimeLabel(meetingTime)
      const label = `Open day view for ${weekday} ${date.getDate()}`
      return {
        day,
        weekday,
        date: String(date.getDate()),
        isToday: day === today,
        isSelected: day === selected,
        hasAllDay: covered.has(day),
        meetingTime: meetingTimeParts(meetingTime),
        label: meetings ? `${label}, meetings ${meetings}` : label,
      }
    })
  })

  protected openDay(day: DayKey): void {
    void this.activity.openDay(day)
  }

  protected openEvent(key: EntryKey): void {
    void this.activity.openEvent(key)
  }

  protected openGroup(keys: readonly EntryKey[]): void {
    void this.activity.openGroup(keys)
  }
}
