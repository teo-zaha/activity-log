import { Component, ElementRef, afterNextRender, computed, inject, viewChild } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { IconButtonComponent } from '../button/icon-button.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityService } from '../../services/activity.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import type { EntryKey } from '../../providers/connection'
import { injectMinuteClock } from '../../utils/clock'
import { dateOfKey, keysWithin, wallClock } from '../../utils/local-day'
import {
  NO_MEETING_TIME,
  meetingTimeByDay,
  meetingTimeLabel,
  meetingTimeParts,
} from '../../utils/meeting-time'
import {
  DEFAULT_SCROLL_HOUR,
  gridHeight,
  layoutAllDay,
  layoutDays,
  nowMarker,
  planCalendar,
} from '../calendar/calendar-grid'
import { CalendarAllDayRowComponent } from '../calendar/calendar-all-day-row/calendar-all-day-row.component'
import { CalendarDayColumnComponent } from '../calendar/calendar-day-column/calendar-day-column.component'
import { CalendarHoursComponent } from '../calendar/calendar-hours/calendar-hours.component'
import { CALENDAR_TIME_ZONE } from '../calendar/calendar-time-zone'
import { injectCalendarZoom } from '../calendar-zoom'
import { NavigationMotionService } from '../../navigation-motion'

/** The most blocks side by side in the day's one column; more go into an overflow. */
const MAX_VISIBLE = 5

@Component({
  selector: 'app-day-view',
  imports: [
    IconButtonComponent,
    ButtonComponent,
    MatIconModule,
    CalendarAllDayRowComponent,
    CalendarDayColumnComponent,
    CalendarHoursComponent,
  ],
  templateUrl: './day-view.component.html',
  styleUrl: './day-view.component.scss',
  // Cross-fades with the calendar view it replaces (`navigation-motion.ts`, `styles.scss`).
  host: {
    '(animate.enter)': "motion.enter($event, 'view')",
    '(animate.leave)': "motion.leave($event, 'view')",
  },
})
export class DayViewComponent {
  protected readonly activity = inject(ActivityService)
  protected readonly motion = inject(NavigationMotionService)
  private readonly isPhone = inject(BreakpointState).phone
  private readonly timeZone = inject(CALENDAR_TIME_ZONE)
  private readonly clock = injectMinuteClock()

  private readonly scrollEl = viewChild.required<ElementRef<HTMLElement>>('scrollEl')
  private readonly gridEl = viewChild.required<ElementRef<HTMLElement>>('gridEl')
  protected readonly zoom = injectCalendarZoom(this.scrollEl, this.gridEl)

  constructor() {
    afterNextRender({
      write: () => {
        this.scrollEl().nativeElement.scrollTop = DEFAULT_SCROLL_HOUR * this.activity.hourHeight()
      },
    })
  }

  protected readonly gridHeight = computed(() => gridHeight(this.activity.hourHeight()))

  protected readonly isToday = computed(
    () => this.activity.selectedDay() === wallClock(this.clock(), this.timeZone).day
  )

  /** `Tuesday, 17 November 2026`; `Tue, 17 Nov 2026` on a phone. */
  protected readonly title = computed(() => {
    const compact = this.isPhone()
    return dateOfKey(this.activity.selectedDay()).toLocaleDateString('en-GB', {
      weekday: compact ? 'short' : 'long',
      day: 'numeric',
      month: compact ? 'short' : 'long',
      year: 'numeric',
    })
  })

  /** The current-time line, while the shown day is today. */
  protected readonly nowLine = computed(() => {
    const marker = nowMarker(this.clock(), this.activity.hourHeight(), this.timeZone)
    return marker.day === this.activity.selectedDay() ? marker : null
  })

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

  private readonly plan = computed(() =>
    planCalendar([this.activity.selectedDay()], this.activity.calendarEntries(), this.timeZone)
  )

  protected readonly allDay = computed(() => layoutAllDay(this.plan()))

  protected readonly column = computed(
    () => layoutDays(this.plan(), this.activity.hourHeight(), MAX_VISIBLE)[0]
  )

  protected readonly isEmpty = computed(
    () => this.allDay().bars.length === 0 && this.column().items.length === 0
  )

  protected openEvent(key: EntryKey): void {
    void this.activity.openEvent(key)
  }

  protected openGroup(keys: readonly EntryKey[]): void {
    void this.activity.openGroup(keys)
  }
}
