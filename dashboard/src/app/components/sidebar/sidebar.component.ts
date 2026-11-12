import { Component, computed, inject } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import type { Source } from '../../generated/activity'
import { VIEW_OPTIONS } from '../../models/view-option'
import { ActivityService } from '../../services/activity.service'
import type { ProviderViewModel } from '../../services/types'
import { injectBreakpointDown } from '../../utils/breakpoints'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { StatusDotComponent } from '../ui-status/status-dot.component'

@Component({
  selector: 'app-sidebar',
  imports: [MatIconModule, MatTooltipModule, ProviderIconComponent, StatusDotComponent],
  templateUrl: './sidebar.component.html',
  styleUrl: './sidebar.component.scss',
})
export class SidebarComponent {
  protected readonly activity = inject(ActivityService)
  private readonly settings = inject(SettingsDialogLauncher)
  private readonly belowTablet = injectBreakpointDown('tablet')

  protected readonly views = VIEW_OPTIONS
  protected readonly isPhone = injectBreakpointDown('phone')
  protected readonly isRail = computed(() => this.belowTablet() && !this.isPhone())

  protected readonly issuesTooltip = computed(() =>
    this.activity
      .unusableProviders()
      .map(provider => `${provider.label}: ${provider.info.reason ?? 'Unavailable'}`)
      .join('\n')
  )

  protected readonly issuesAriaLabel = computed(() => {
    const count = this.activity.unusableProviders().length
    return `${count} ${count === 1 ? 'source' : 'sources'} unavailable. ${this.issuesTooltip()}. Open settings`
  })

  protected isActive(id: Source): boolean {
    return this.activity.activeSources().has(id)
  }

  protected sourceTooltip(provider: ProviderViewModel): string {
    const parts = this.isRail() ? [`${provider.label} · ${provider.count}`] : []
    if (provider.tooltip) parts.push(provider.tooltip)
    return parts.join('\n')
  }

  protected sourceAriaLabel(provider: ProviderViewModel): string {
    const reason = provider.tooltip ? `, ${provider.tooltip}` : ''
    return `${provider.label}, ${provider.count} events${reason}`
  }

  protected openSettings(source?: Source): void {
    this.settings.open(source)
  }
}
