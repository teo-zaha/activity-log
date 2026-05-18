import { Component, inject, computed, ChangeDetectionStrategy } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { MatButtonModule } from '@angular/material/button'
import { MatIconModule } from '@angular/material/icon'
import { MatFormFieldModule } from '@angular/material/form-field'
import { MatInputModule } from '@angular/material/input'
import { MatSelectModule } from '@angular/material/select'
import { MatTooltipModule } from '@angular/material/tooltip'
import { MatDatepickerModule } from '@angular/material/datepicker'
import { MatNativeDateModule } from '@angular/material/core'
import { ActivityService } from '../../services/activity.service'
import { ThemeService } from '../../services/theme.service'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'

@Component({
  selector: 'app-header',
  imports: [
    FormsModule,
    MatButtonModule,
    MatIconModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    MatTooltipModule,
    MatDatepickerModule,
    MatNativeDateModule,
  ],
  templateUrl: './header.component.html',
  styleUrl: './header.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class HeaderComponent {
  protected readonly activity = inject(ActivityService)
  protected readonly theme = inject(ThemeService)
  protected readonly runtime = inject(RuntimeEnvironmentService)

  protected readonly dateFromObj = computed(() => this.parseDateStr(this.activity.dateFrom()))
  protected readonly dateToObj = computed(() => this.parseDateStr(this.activity.dateTo()))

  protected apply(): void {
    void this.activity.collect(this.activity.dateFrom(), this.activity.dateTo())
  }

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
