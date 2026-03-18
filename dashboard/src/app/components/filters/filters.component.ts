import { Component, inject } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { ActivityService } from '../../services/activity.service'
import { ThemeService, THEMES } from '../../services/theme.service'

interface SourceFilter {
  id: string
  label: string
  icon: string
  color: string
}

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

  protected readonly sources: SourceFilter[] = [
    { id: 'git',     label: 'Git',     icon: 'commit',             color: '#1565c0' },
    { id: 'gitlab',  label: 'GitLab',  icon: 'code',               color: '#c2390b' },
    { id: 'jira',    label: 'Jira',    icon: 'task_alt',           color: '#004baa' },
    { id: 'outlook', label: 'Outlook', icon: 'event',              color: '#005a9e' },
    { id: 'system',  label: 'System',  icon: 'power_settings_new', color: '#374151' },
  ]

  protected isActive(id: string): boolean {
    return this.activity.activeSources().has(id)
  }
}
