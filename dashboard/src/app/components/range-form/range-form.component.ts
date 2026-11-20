import { Component, computed, inject, linkedSignal } from '@angular/core'
import { form, FormField, FormRoot } from '@angular/forms/signals'
import { MatDatepickerModule } from '@angular/material/datepicker'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { ActivityService } from '../../services/activity.service'
import { dateOfKey, localDateKey, type DayKey } from '../../utils/local-day'
import { ButtonComponent } from '../button/button.component'

interface DateRange {
  readonly from: Date | null
  readonly to: Date | null
}

/** The collection range, From and To, and Collect. */
@Component({
  selector: 'app-range-form',
  imports: [
    ButtonComponent,
    FormField,
    FormRoot,
    MatDatepickerModule,
    MatIconModule,
    MatTooltipModule,
  ],
  templateUrl: './range-form.component.html',
  styleUrl: './range-form.component.scss',
})
export class RangeFormComponent {
  protected readonly activity = inject(ActivityService)

  protected readonly dateFromObj = computed(() => this.parseDateStr(this.activity.dateFrom()))
  protected readonly dateToObj = computed(() => this.parseDateStr(this.activity.dateTo()))

  /**
   * What the pickers show while a date is being typed. It only reaches the activity filters once
   * the input commits (`dateChange`), so half-typed dates never filter the timeline.
   */
  protected readonly range = linkedSignal<DateRange>(() => ({
    from: this.dateFromObj(),
    to: this.dateToObj(),
  }))

  protected readonly rangeForm = form(this.range, {
    submission: {
      // The pickers' own min/max validators only grey out dates; they never blocked collecting.
      ignoreValidators: 'all',
      action: async () => {
        const [from, to] = [this.activity.dateFrom(), this.activity.dateTo()]
        if (from && to) await this.activity.collect(from, to)
      },
    },
  })

  protected parseDateStr(day: DayKey | null): Date | null {
    return day === null ? null : dateOfKey(day)
  }

  /** The day a picker holds; none for no date or one it could not read. */
  protected fmtDate(date: Date | null): DayKey | null {
    return date && !Number.isNaN(date.getTime()) ? localDateKey(date) : null
  }
}
