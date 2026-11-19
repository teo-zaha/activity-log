import { Component, computed, inject, linkedSignal } from '@angular/core'
import { form, FormField, FormRoot } from '@angular/forms/signals'
import { MatTooltipModule } from '@angular/material/tooltip'
import { MatDatepickerModule } from '@angular/material/datepicker'
import { MatNativeDateModule } from '@angular/material/core'
import { MatIconModule } from '@angular/material/icon'
import { Router } from '@angular/router'
import { IconButtonComponent } from '../button/icon-button.component'
import { ButtonComponent } from '../button/button.component'
import { ActivityService } from '../../services/activity.service'
import { WindowChromeService } from '../../services/window-chrome.service'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { appLink, injectShareLink } from '../share/share-link'
import { dateOfKey, localDateKey } from '../../utils/local-day'

interface DateRange {
  readonly from: Date | null
  readonly to: Date | null
}

@Component({
  selector: 'app-header',
  imports: [
    IconButtonComponent,
    ButtonComponent,
    MatIconModule,
    FormField,
    FormRoot,
    MatTooltipModule,
    MatDatepickerModule,
    MatNativeDateModule,
  ],
  templateUrl: './header.component.html',
  styleUrl: './header.component.scss',
})
export class HeaderComponent {
  protected readonly activity = inject(ActivityService)
  private readonly chrome = inject(WindowChromeService)
  private readonly router = inject(Router)
  /** On a phone the settings are a tab, without the range that filters the views. */
  protected readonly settingsTab = inject(SettingsDialogLauncher).pageOpen
  private readonly shareLink = injectShareLink()

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

  protected parseDateStr(s: string): Date | null {
    if (!s) return null
    return dateOfKey(s)
  }

  protected fmtDate(d: Date | null): string {
    return d ? localDateKey(d) : ''
  }

  protected exportMd(): void {
    void this.activity.exportMarkdown()
  }

  protected exportJson(): void {
    void this.activity.exportJson()
  }

  /** Shares the link to this view, with the details open in it, if any. */
  protected shareView(): Promise<void> {
    return this.shareLink(appLink(this.router, ['q', 'from', 'to', 'event', 'events']))
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

  protected startWindowDrag(event: MouseEvent): Promise<void> {
    return this.chrome.startDrag(event)
  }

  protected toggleWindowMaximize(event: MouseEvent): Promise<void> {
    return this.chrome.toggleMaximize(event)
  }
}
