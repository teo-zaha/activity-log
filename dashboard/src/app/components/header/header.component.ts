import { Component, inject, computed } from '@angular/core'
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

@Component({
  selector: 'app-header',
  standalone: true,
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
})
export class HeaderComponent {
  protected readonly activity = inject(ActivityService)
  protected readonly theme = inject(ThemeService)

  protected readonly dateFromObj = computed(() => this.parseDateStr(this.activity.dateFrom()))
  protected readonly dateToObj   = computed(() => this.parseDateStr(this.activity.dateTo()))


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
    const md = this.activity.exportMarkdown()
    const blob = new Blob([md], { type: 'text/markdown' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `activity-${this.activity.dateFrom()}-${this.activity.dateTo()}.md`
    a.click()
    URL.revokeObjectURL(a.href)
  }
}
