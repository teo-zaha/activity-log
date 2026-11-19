import { Component, computed, inject } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { RouterLink, RouterLinkActive } from '@angular/router'
import { AutoAnimateDirective } from '../auto-animate/auto-animate.directive'
import { mediaQuery } from '@signality/core/browser/media-query'
import type { ConnectionKey } from '../../providers/connection'
import { VIEW_OPTIONS } from '../../models/view-option'
import { ActivityService } from '../../services/activity.service'
import type { ProviderViewModel, ViewMode } from '../../services/types'
import { viewCommands } from '../../utils/app-url'
import { downQuery } from '../../utils/breakpoints'
import { IconButtonComponent } from '../button/icon-button.component'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import type { SettingsTarget } from '../settings-dialog/settings-dialog.types'
import { StatusDotComponent } from '../ui-status/status-dot.component'

@Component({
  selector: 'app-sidebar',
  imports: [
    AutoAnimateDirective,
    IconButtonComponent,
    MatIconModule,
    MatTooltipModule,
    RouterLink,
    RouterLinkActive,
    ProviderIconComponent,
    StatusDotComponent,
  ],
  templateUrl: './sidebar.component.html',
  styleUrl: './sidebar.component.scss',
})
export class SidebarComponent {
  protected readonly activity = inject(ActivityService)
  private readonly settings = inject(SettingsDialogLauncher)
  private readonly belowTablet = mediaQuery(downQuery('tablet'))

  protected readonly views = VIEW_OPTIONS
  protected readonly isPhone = mediaQuery(downQuery('phone'))
  protected readonly isRail = computed(() => this.belowTablet() && !this.isPhone())
  /**
   * On a phone the settings are the bottom bar's last tab: its views are not current then, and the
   * source chips, which filter the views, are left out.
   */
  protected readonly settingsTab = this.settings.pageOpen

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

  /** A calendar view opens on the day shown last. */
  protected viewLink(view: ViewMode): string[] {
    return viewCommands(view, this.activity.selectedDay())
  }

  protected isActive(key: ConnectionKey): boolean {
    return this.activity.activeConnections().has(key)
  }

  protected sourceTooltip(provider: ProviderViewModel): string {
    if (provider.info.status === 'checking') return `${provider.label}: checking`
    if (provider.collecting) return `${provider.label}: collecting`
    const parts = this.isRail() ? [`${provider.label} · ${provider.count}`] : []
    if (provider.tooltip) parts.push(provider.tooltip)
    return parts.join('\n')
  }

  protected sourceAriaLabel(provider: ProviderViewModel): string {
    if (provider.info.status === 'checking') return `${provider.label}, checking`
    if (provider.collecting) return `${provider.label}, collecting`
    const reason = provider.tooltip ? `, ${provider.tooltip}` : ''
    return `${provider.label}, ${provider.count} events${reason}`
  }

  /** Another OS cannot be fixed by checking again; any other problem can be. */
  protected canRecheck(provider: ProviderViewModel): boolean {
    return provider.info.problem !== 'unsupported'
  }

  protected recheck(provider: ProviderViewModel): void {
    void this.activity.recheckConnection(provider.info.connection)
  }

  /** Tapping the open tab again keeps the page it is on. */
  protected showSettingsTab(): void {
    if (!this.settingsTab()) this.openSettings()
  }

  protected openSettings(target?: SettingsTarget): void {
    this.settings.open(target)
  }
}
