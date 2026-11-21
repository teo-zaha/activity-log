import { Component, computed, inject } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import type { ConnectionKey } from '../../providers/connection'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection } from '../../services/activity-selection.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import { failureWhile } from '../../services/failure'
import { NoticeService } from '../../services/notice.service'
import type { ProviderViewModel } from '../../services/types'
import { AutoAnimateDirective } from '../auto-animate/auto-animate.directive'
import { IconButtonComponent } from '../button/icon-button.component'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import type { SettingsTarget } from '../settings-dialog/settings-dialog.types'
import { StatusDotComponent } from '../ui-status/status-dot.component'

/**
 * The source chips that filter the views, Add source, and the sources that are unavailable: a
 * column in the sidebar, icons in the rail, a row of chips on a phone.
 */
@Component({
  selector: 'app-source-list',
  imports: [
    AutoAnimateDirective,
    IconButtonComponent,
    MatIconModule,
    MatTooltipModule,
    ProviderIconComponent,
    StatusDotComponent,
  ],
  templateUrl: './source-list.component.html',
  styleUrl: './source-list.component.scss',
  host: { '[class.is-rail]': 'isRail()' },
})
export class SourceListComponent {
  protected readonly data = inject(ActivityDataService)
  protected readonly selection = inject(ActivitySelection)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly notices = inject(NoticeService)
  private readonly settings = inject(SettingsDialogLauncher)
  private readonly breakpoints = inject(BreakpointState)

  protected readonly isPhone = this.breakpoints.phone
  protected readonly isRail = this.breakpoints.rail

  protected readonly issuesTooltip = computed(() =>
    this.data
      .unusableProviders()
      .map(provider => `${provider.label}: ${provider.info.reason ?? 'Unavailable'}`)
      .join('\n')
  )

  protected readonly issuesAriaLabel = computed(() => {
    const count = this.data.unusableProviders().length
    return `${count} ${count === 1 ? 'source' : 'sources'} unavailable. ${this.issuesTooltip()}. Open settings`
  })

  protected isActive(key: ConnectionKey): boolean {
    return this.viewState.activeConnections().has(key)
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

  /** The backend says which problems a new check can fix (`ProviderInfo.recheckable`). */
  protected canRecheck(provider: ProviderViewModel): boolean {
    return provider.info.recheckable
  }

  protected async recheck(provider: ProviderViewModel): Promise<void> {
    try {
      await this.data.recheckConnection(provider.info.connection)
    } catch (error) {
      this.notices.show({
        origin: 'recheck',
        failure: failureWhile(`Checking ${provider.label} again`, error),
      })
    }
  }

  protected openSettings(target: SettingsTarget): void {
    this.settings.open(target)
  }
}
