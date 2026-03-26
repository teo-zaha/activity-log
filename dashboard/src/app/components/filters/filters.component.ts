import { Component, inject } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import type { Source } from '../../generated/activity'
import { ActivityService } from '../../services/activity.service'
import { ThemeService, THEMES } from '../../services/theme.service'

@Component({
  selector: 'app-filters',
  standalone: true,
  imports: [MatIconModule, MatTooltipModule],
  templateUrl: './filters.component.html',
  styleUrl: './filters.component.scss',
})
export class FiltersComponent {
  protected readonly activity = inject(ActivityService)
  protected readonly theme = inject(ThemeService)
  protected readonly themes = THEMES

  protected isActive(id: Source): boolean {
    return this.activity.activeSources().has(id)
  }
}
