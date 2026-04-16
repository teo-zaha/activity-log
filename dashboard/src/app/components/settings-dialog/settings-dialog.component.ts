import { Component, computed, inject, signal } from '@angular/core'
import { MatDialogModule, MatDialogRef } from '@angular/material/dialog'
import { MatIconModule } from '@angular/material/icon'
import type { Source } from '../../generated/activity'
import { getProviderAdapter } from '../../providers/provider-adapter'
import { ActivityService } from '../../services/activity.service'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SettingsDialogMobileTabsComponent } from './settings-dialog-mobile-tabs.component'
import { SettingsDialogNavComponent } from './settings-dialog-nav.component'
import { SettingsGeneralPaneComponent } from './settings-general-pane.component'
import { SettingsSourcePaneComponent } from './settings-source-pane.component'
import type { SettingsProviderRow, SettingsSelection } from './settings-dialog.types'

@Component({
  selector: 'app-settings-dialog',
  standalone: true,
  imports: [
    MatDialogModule,
    MatIconModule,
    ProviderIconComponent,
    SettingsDialogMobileTabsComponent,
    SettingsDialogNavComponent,
    SettingsGeneralPaneComponent,
    SettingsSourcePaneComponent,
  ],
  templateUrl: './settings-dialog.component.html',
  styleUrl: './settings-dialog.component.scss',
})
export class SettingsDialogComponent {
  protected readonly activity = inject(ActivityService)
  protected readonly dialogRef = inject(MatDialogRef<SettingsDialogComponent>)

  protected readonly selection = signal<SettingsSelection>('general')
  protected readonly providerRows = computed<SettingsProviderRow[]>(() =>
    this.activity.providerInfo().map(info => {
      const adapter = getProviderAdapter(info.id)
      return {
        info,
        label: adapter.label,
        icon: adapter.providerIcon,
        tooltip: adapter.getStatusTooltip(info),
      }
    })
  )
  protected readonly selectedProvider = computed<SettingsProviderRow | null>(() => {
    const selection = this.selection()
    if (selection === 'general') {
      return null
    }

    const source = selection.slice('source:'.length) as Source
    return this.providerRows().find(provider => provider.info.id === source) ?? null
  })

  protected selectGeneral(): void {
    this.selection.set('general')
  }

  protected selectSource(source: Source): void {
    this.selection.set(`source:${source}`)
  }

  protected close(): void {
    this.dialogRef.close()
  }
}
