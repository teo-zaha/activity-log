import { Component, input, output } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import type { EntryKey } from '../../../providers/connection'
import type { EntryUiAdapter } from '../../../providers/types'
import type { DayColumn, NowMarker } from '../calendar-grid'

/**
 * One day of the calendar grid: its blocks and overflow, laid out by `layoutDays`, and the
 * current-time line on today. The view places it and draws the hours and the time label.
 */
@Component({
  selector: 'app-calendar-day-column',
  imports: [MatIconModule],
  templateUrl: './calendar-day-column.component.html',
  styleUrl: './calendar-day-column.component.scss',
})
export class CalendarDayColumnComponent {
  readonly column = input.required<DayColumn<EntryUiAdapter>>()
  /** The current time, on today's column only. */
  readonly now = input<NowMarker | null>(null)
  readonly openEvent = output<EntryKey>()
  readonly openGroup = output<readonly EntryKey[]>()
}
