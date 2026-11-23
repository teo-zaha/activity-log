import { Component, ElementRef, computed, inject, viewChild } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { IconButtonComponent } from '../button/icon-button.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection } from '../../services/activity-selection.service'
import { ActivityViewStateService, type DayRange } from '../../services/activity-view-state.service'
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
import { calendarEmptyHint } from '../calendar/calendar-empty-hint'
import { CalendarHintComponent } from '../calendar/calendar-hint/calendar-hint.component'
import { collectedDays } from '../calendar/collected-days'
import { injectMinuteClock } from '../../utils/clock'
import { dateOfKey, wallClock, type DayKey } from '../../utils/local-day'
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
import { restoresScroll } from '../../services/views-scroller.service'
import { KeyTagComponent } from '../shortcut/key-tag.component'
import { ShortcutDirective } from '../shortcut/shortcut.directive'

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
    CalendarHintComponent,
    KeyTagComponent,
    ShortcutDirective,
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
  protected readonly viewState = inject(ActivityViewStateService)
  private readonly selection = inject(ActivitySelection)
  private readonly data = inject(ActivityDataService)
  protected readonly motion = inject(NavigationMotionService)
  private readonly timeZone = inject(CALENDAR_TIME_ZONE)

  private readonly scrollEl = viewChild.required<ElementRef<HTMLElement>>('scrollEl')
  private readonly gridEl = viewChild.required<ElementRef<HTMLElement>>('gridEl')
  protected readonly zoom = injectCalendarZoom(this.scrollEl, this.gridEl)
  private readonly clock = injectMinuteClock()
  private readonly today = computed(() => wallClock(this.clock(), this.timeZone).day)

  constructor() {
    // A new navigation shows the grid from the default hour, Back and Forward where it was left.
    restoresScroll(this.scrollEl, () => DEFAULT_SCROLL_HOUR * this.viewState.hourHeight())
  }

  protected readonly days = computed<DayKey[]>(() =>
    this.viewState.currentView() === 'workweek'
      ? this.viewState.workWeekDays()
      : this.viewState.weekDays()
  )

  protected readonly gridHeight = computed(() => gridHeight(this.viewState.hourHeight()))

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
    const marker = nowMarker(this.clock(), this.viewState.hourHeight(), this.timeZone)
    return this.days().includes(marker.day) ? marker : null
  })

  private readonly plan = computed(() =>
    planCalendar(this.days(), this.selection.calendarEntries(), this.timeZone)
  )

  protected readonly allDay = computed(() => layoutAllDay(this.plan()))

  protected readonly columns = computed<DayColumn<EntryUiAdapter>[]>(() =>
    layoutDays(
      this.plan(),
      this.viewState.hourHeight(),
      this.viewState.currentView() === 'workweek' ? 4 : 3
    )
  )

  /** The shown days whose entries were read, see `collectedDays`. */
  private readonly collectedDays = computed(() =>
    collectedDays(
      this.days(),
      this.viewState.dateFrom(),
      this.viewState.dateTo(),
      this.selection.rangeCoverage()
    )
  )

  /** Why an empty week is empty, see `calendarEmptyHint`; null with entries. */
  protected readonly emptyHint = computed(() => {
    const empty =
      this.allDay().bars.length === 0 && this.columns().every(column => column.items.length === 0)
    return empty
      ? calendarEmptyHint({
          unit: 'week',
          days: this.days(),
          from: this.viewState.dateFrom(),
          to: this.viewState.dateTo(),
          coverage: this.selection.rangeCoverage(),
          loading: this.data.isLoading(),
          status: this.data.collectionStatus(),
          hasSources: this.data.anySourceReady(),
          anyShown: this.viewState.activeConnections().size > 0,
        })
      : null
  })

  /** Only for the collected days: a day outside them has meetings that were not read. */
  private readonly meetingTimes = computed(() =>
    meetingTimeByDay(this.selection.calendarEntries(), this.collectedDays())
  )

  private readonly weekMeetingTime = computed(() => sumMeetingTime(this.meetingTimes().values()))

  protected readonly weekMeetingTimeParts = computed(() => meetingTimeParts(this.weekMeetingTime()))

  protected readonly weekMeetingTimeLabel = computed(() => meetingTimeLabel(this.weekMeetingTime()))

  protected readonly weekDays = computed<WeekDay[]>(() => {
    const [today, selected, covered] = [
      this.today(),
      this.viewState.selectedDay(),
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

  protected collectRange(range: DayRange): void {
    void this.data.collectRange(range)
  }

  protected openDay(day: DayKey): void {
    void this.viewState.openDay(day)
  }

  protected openEvent(key: EntryKey): void {
    void this.viewState.openEvent(key)
  }

  protected openGroup(keys: readonly EntryKey[]): void {
    void this.viewState.openGroup(keys)
  }
}
