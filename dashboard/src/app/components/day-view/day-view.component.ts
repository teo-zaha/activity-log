import { Component, ElementRef, afterNextRender, computed, inject, viewChild } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { IconButtonComponent } from '../button/icon-button.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection } from '../../services/activity-selection.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import type { EntryKey } from '../../providers/connection'
import { injectMinuteClock } from '../../utils/clock'
import { dateOfKey, wallClock } from '../../utils/local-day'
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
import { calendarEmptyHint } from '../calendar/calendar-empty-hint'
import { CalendarHintComponent } from '../calendar/calendar-hint/calendar-hint.component'
import { collectedDays } from '../calendar/collected-days'
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
    CalendarHintComponent,
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
  protected readonly viewState = inject(ActivityViewStateService)
  private readonly selection = inject(ActivitySelection)
  private readonly data = inject(ActivityDataService)
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
        this.scrollEl().nativeElement.scrollTop = DEFAULT_SCROLL_HOUR * this.viewState.hourHeight()
      },
    })
  }

  protected readonly gridHeight = computed(() => gridHeight(this.viewState.hourHeight()))

  protected readonly isToday = computed(
    () => this.viewState.selectedDay() === wallClock(this.clock(), this.timeZone).day
  )

  /** `Tuesday, 17 November 2026`; `Tue, 17 Nov 2026` on a phone. */
  protected readonly title = computed(() => {
    const compact = this.isPhone()
    return dateOfKey(this.viewState.selectedDay()).toLocaleDateString('en-GB', {
      weekday: compact ? 'short' : 'long',
      day: 'numeric',
      month: compact ? 'short' : 'long',
      year: 'numeric',
    })
  })

  /** The current-time line, while the shown day is today. */
  protected readonly nowLine = computed(() => {
    const marker = nowMarker(this.clock(), this.viewState.hourHeight(), this.timeZone)
    return marker.day === this.viewState.selectedDay() ? marker : null
  })

  /** Whether the day's entries were read, see `collectedDays`. */
  private readonly collected = computed(
    () =>
      collectedDays(
        [this.viewState.selectedDay()],
        this.viewState.dateFrom(),
        this.viewState.dateTo(),
        this.selection.rangeCoverage()
      ).length > 0
  )

  /** Only for a collected day: outside the collection its meetings were not read. */
  private readonly meetingTime = computed(() => {
    if (!this.collected()) return NO_MEETING_TIME
    const day = this.viewState.selectedDay()
    return meetingTimeByDay(this.selection.calendarEntries(), [day]).get(day) ?? NO_MEETING_TIME
  })

  protected readonly meetingTimeParts = computed(() => meetingTimeParts(this.meetingTime()))
  protected readonly meetingTimeLabel = computed(() => meetingTimeLabel(this.meetingTime()))

  private readonly plan = computed(() =>
    planCalendar([this.viewState.selectedDay()], this.selection.calendarEntries(), this.timeZone)
  )

  protected readonly allDay = computed(() => layoutAllDay(this.plan()))

  protected readonly column = computed(
    () => layoutDays(this.plan(), this.viewState.hourHeight(), MAX_VISIBLE)[0]
  )

  /** Why an empty day is empty, see `calendarEmptyHint`; null with entries. */
  protected readonly emptyHint = computed(() =>
    this.allDay().bars.length === 0 && this.column().items.length === 0
      ? calendarEmptyHint({
          unit: 'day',
          days: [this.viewState.selectedDay()],
          from: this.viewState.dateFrom(),
          to: this.viewState.dateTo(),
          coverage: this.selection.rangeCoverage(),
          loading: this.data.isLoading(),
          status: this.data.collectionStatus(),
          hasSources: this.data.anySourceReady(),
          anyShown: this.viewState.activeConnections().size > 0,
        })
      : null
  )

  protected collectOwnRange(): void {
    void this.data.collectShownRange()
  }

  protected openEvent(key: EntryKey): void {
    void this.viewState.openEvent(key)
  }

  protected openGroup(keys: readonly EntryKey[]): void {
    void this.viewState.openGroup(keys)
  }
}
