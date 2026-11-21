import { Component, input, output } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import type { EntryKey } from '../../../providers/connection'
import type { EntryUiAdapter } from '../../../providers/types'
import type { AllDayRow } from '../calendar-grid'

/** The bars of meetings past midnight above the grid, laid out by `layoutAllDay`. */
@Component({
  selector: 'app-calendar-all-day-row',
  imports: [MatIconModule],
  templateUrl: './calendar-all-day-row.component.html',
  styleUrl: './calendar-all-day-row.component.scss',
  host: { '[style.height.px]': 'row().height' },
})
export class CalendarAllDayRowComponent {
  readonly row = input.required<AllDayRow<EntryUiAdapter>>()
  readonly openEvent = output<EntryKey>()
}
