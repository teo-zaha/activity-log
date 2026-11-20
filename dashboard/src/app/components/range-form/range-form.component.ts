import { Component, DestroyRef, computed, inject, linkedSignal } from '@angular/core'
import { form, FormField, FormRoot } from '@angular/forms/signals'
import { MatDatepickerModule } from '@angular/material/datepicker'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { ActivityService } from '../../services/activity.service'
import { ShortcutService } from '../../services/shortcut.service'
import { dateOfKey, localDateKey, type DayKey } from '../../utils/local-day'
import { ButtonComponent } from '../button/button.component'
import { KeyTagComponent } from '../shortcut/key-tag.component'
import { ShortcutDirective } from '../shortcut/shortcut.directive'

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
    KeyTagComponent,
    MatDatepickerModule,
    MatIconModule,
    MatTooltipModule,
    ShortcutDirective,
  ],
  templateUrl: './range-form.component.html',
  styleUrl: './range-form.component.scss',
})
export class RangeFormComponent {
  protected readonly activity = inject(ActivityService)
  private readonly shortcuts = inject(ShortcutService)
  /** While a picker's calendar is open, it owns the keyboard; the app's keys wait. */
  private closePickerLayer: (() => void) | null = null

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
        await this.commitTypedRange()
        await this.activity.collectShownRange()
      },
    },
  })

  constructor() {
    inject(DestroyRef).onDestroy(() => this.pickerClosed())
  }

  protected pickerOpened(): void {
    this.closePickerLayer ??= this.shortcuts.openLayer({ kind: 'modal' })
  }

  protected pickerClosed(): void {
    this.closePickerLayer?.()
    this.closePickerLayer = null
  }

  /**
   * Takes a date still being typed (its field has focus, so it has not committed) into the range,
   * when it is a whole, real date: Collect, by click, Enter or shortcut, collects what the fields
   * show. A date that is none leaves the range as it was.
   */
  private async commitTypedRange(): Promise<void> {
    const { from, to } = this.range()
    const [fromDay, toDay] = [this.fmtDate(from), this.fmtDate(to)]
    if (fromDay !== null && fromDay !== this.activity.dateFrom()) {
      await this.activity.setDateFrom(fromDay)
    }
    if (toDay !== null && toDay !== this.activity.dateTo()) {
      await this.activity.setDateTo(toDay)
    }
  }

  protected parseDateStr(day: DayKey | null): Date | null {
    return day === null ? null : dateOfKey(day)
  }

  /** The day a picker holds; none for no date or one it could not read. */
  protected fmtDate(date: Date | null): DayKey | null {
    return date && !Number.isNaN(date.getTime()) ? localDateKey(date) : null
  }
}
