import { Component, input } from '@angular/core'
import { HOUR_ROWS } from '../calendar-grid'

/**
 * The hour rows of the calendar grid: a label in the gutter and a line across, positioned in the
 * grid that holds them (the host takes no box of its own).
 */
@Component({
  selector: 'app-calendar-hours',
  templateUrl: './calendar-hours.component.html',
  styleUrl: './calendar-hours.component.scss',
})
export class CalendarHoursComponent {
  readonly hourHeight = input.required<number>()
  /** The hour whose label the current-time label covers, hidden; or null. */
  readonly coveredHour = input<number | null>(null)
  protected readonly rows = HOUR_ROWS
}
