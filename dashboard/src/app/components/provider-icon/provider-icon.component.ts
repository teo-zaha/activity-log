import { Component, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import type { Source } from '../../generated/activity'

@Component({
  selector: 'app-provider-icon',
  imports: [MatIconModule],
  templateUrl: './provider-icon.component.html',
  styleUrl: './provider-icon.component.scss',
})
export class ProviderIconComponent {
  readonly source = input.required<Source>()
  readonly fallbackIcon = input('help')

  protected brandIconUrl(): string | null {
    switch (this.source()) {
      case 'git':
        return 'assets/provider-icons/git.svg'
      case 'gitlab':
        return 'assets/provider-icons/gitlab.svg'
      case 'jira':
        return 'assets/provider-icons/jira.svg'
      case 'outlook':
        return 'assets/provider-icons/outlook.svg'
      default:
        return null
    }
  }
}
