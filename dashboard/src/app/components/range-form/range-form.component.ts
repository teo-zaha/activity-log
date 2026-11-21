import {
  Component,
  DestroyRef,
  type ElementRef,
  computed,
  effect,
  inject,
  linkedSignal,
  signal,
  viewChild,
} from '@angular/core'
import { form, FormField, FormRoot } from '@angular/forms/signals'
import { MatDatepickerModule } from '@angular/material/datepicker'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { elementFocusWithin } from '@signality/core/elements/element-focus-within'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
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
  protected readonly data = inject(ActivityDataService)
  protected readonly viewState = inject(ActivityViewStateService)
  private readonly shortcuts = inject(ShortcutService)
  /** While a picker's calendar is open, it owns the keyboard; the app's keys wait. */
  private closePickerLayer: (() => void) | null = null

  protected readonly dateFromObj = computed(() => this.parseDateStr(this.viewState.dateFrom()))
  protected readonly dateToObj = computed(() => this.parseDateStr(this.viewState.dateTo()))

  private readonly fields = viewChild<ElementRef<HTMLElement>>('fields')
  private readonly fieldFocused = elementFocusWithin(this.fields)
  private readonly pickerOpen = signal(false)
  /** A date field has focus or a picker is open: what is typed there is the user's. */
  private readonly editing = computed(() => this.fieldFocused() || this.pickerOpen())

  /**
   * What the pickers show while a date is being typed. It only reaches the activity filters once
   * the input commits (`dateChange`), so half-typed dates never filter the timeline. While the
   * range is edited it keeps what is typed, whatever range comes meanwhile; once the editing
   * ends it shows the range again.
   */
  protected readonly range = linkedSignal<{ shown: DateRange; editing: boolean }, DateRange>({
    source: () => ({
      shown: { from: this.dateFromObj(), to: this.dateToObj() },
      editing: this.editing(),
    }),
    computation: ({ shown, editing }, previous) =>
      editing && previous !== undefined ? previous.value : shown,
  })

  protected readonly rangeForm = form(this.range, {
    submission: {
      // The pickers' own min/max validators only grey out dates; they never blocked collecting.
      ignoreValidators: 'all',
      action: async () => {
        await this.commitTypedRange()
        await this.data.collectShownRange()
      },
    },
  })

  constructor() {
    effect(() => this.viewState.setRangeEditing(this.editing()))
    inject(DestroyRef).onDestroy(() => {
      this.pickerClosed()
      this.viewState.setRangeEditing(false)
    })
  }

  protected pickerOpened(): void {
    this.closePickerLayer ??= this.shortcuts.openLayer({ kind: 'modal' })
    this.pickerOpen.set(true)
  }

  protected pickerClosed(): void {
    this.closePickerLayer?.()
    this.closePickerLayer = null
    this.pickerOpen.set(false)
  }

  /**
   * Takes a date still being typed (its field has focus, so it has not committed) into the range,
   * when it is a whole, real date: Collect, by click, Enter or shortcut, collects what the fields
   * show. A date that is none leaves the range as it was.
   */
  private async commitTypedRange(): Promise<void> {
    const { from, to } = this.range()
    const [fromDay, toDay] = [this.fmtDate(from), this.fmtDate(to)]
    if (fromDay !== null && fromDay !== this.viewState.dateFrom()) {
      await this.viewState.setDateFrom(fromDay)
    }
    if (toDay !== null && toDay !== this.viewState.dateTo()) {
      await this.viewState.setDateTo(toDay)
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
