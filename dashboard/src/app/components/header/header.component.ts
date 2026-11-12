import { Component, computed, inject, linkedSignal } from '@angular/core'
import { form, FormField, FormRoot } from '@angular/forms/signals'
import { MatTooltipModule } from '@angular/material/tooltip'
import { MatDatepickerModule } from '@angular/material/datepicker'
import { MatNativeDateModule } from '@angular/material/core'
import { ActivityService } from '../../services/activity.service'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'

interface DateRange {
  readonly from: Date | null
  readonly to: Date | null
}

@Component({
  selector: 'app-header',
  imports: [FormField, FormRoot, MatTooltipModule, MatDatepickerModule, MatNativeDateModule],
  templateUrl: './header.component.html',
  styleUrl: './header.component.scss',
})
export class HeaderComponent {
  protected readonly activity = inject(ActivityService)
  protected readonly runtime = inject(RuntimeEnvironmentService)

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
        await this.activity.collect(this.activity.dateFrom(), this.activity.dateTo())
      },
    },
  })

  protected readonly rangeLabel = computed(() => {
    if (!this.activity.meta()) return null
    const count = this.activity.filteredEntries().length
    return `${count} ${count === 1 ? 'event' : 'events'} shown`
  })

  protected parseDateStr(s: string): Date | null {
    if (!s) return null
    return new Date(s + 'T12:00:00')
  }

  protected fmtDate(d: Date | null): string {
    if (!d) return ''
    const y = d.getFullYear()
    const m = String(d.getMonth() + 1).padStart(2, '0')
    const day = String(d.getDate()).padStart(2, '0')
    return `${y}-${m}-${day}`
  }

  protected exportMd(): void {
    void this.activity.exportMarkdown()
  }

  protected exportJson(): void {
    void this.activity.exportJson()
  }

  protected startHeaderDrag(event: MouseEvent): void {
    if (this.isDragSurface(event)) void this.startWindowDrag(event)
  }

  protected toggleHeaderMaximize(event: MouseEvent): void {
    if (this.isDragSurface(event)) void this.toggleWindowMaximize(event)
  }

  private isDragSurface(event: MouseEvent): boolean {
    const target = event.target
    return (
      target === event.currentTarget ||
      (target instanceof Element && target.closest('.brand') !== null)
    )
  }

  protected async startWindowDrag(event: MouseEvent): Promise<void> {
    if (!this.runtime.isTauri() || event.button !== 0) {
      return
    }

    event.preventDefault()

    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    await getCurrentWindow().startDragging()
  }

  protected async toggleWindowMaximize(event: MouseEvent): Promise<void> {
    if (!this.runtime.isTauri() || event.button !== 0) {
      return
    }

    event.preventDefault()

    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    await getCurrentWindow().toggleMaximize()
  }
}
