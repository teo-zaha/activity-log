import { Component, computed, input, output } from '@angular/core'
import type { DayRange } from '../../../services/activity-view-state.service'
import { ButtonComponent } from '../../button/button.component'
import { calendarHintText, type CalendarEmptyHint } from '../calendar-empty-hint'

/**
 * The hint of an empty calendar view, in the view's sticky `.u-floating-hint`: a pill with its
 * text, or a card that also names the days on screen or offers to collect this tab's range. Only
 * its text is a live region, never the button.
 */
@Component({
  selector: 'app-calendar-hint',
  imports: [ButtonComponent],
  templateUrl: './calendar-hint.component.html',
  styleUrl: './calendar-hint.component.scss',
  host: { class: 'calendar-hint' },
})
export class CalendarHintComponent {
  readonly hint = input.required<CalendarEmptyHint>()
  /** Collect the range the hint offers, as this tab's own. */
  readonly collect = output<DayRange>()

  protected readonly text = computed(() => calendarHintText(this.hint()))
}
