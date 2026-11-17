import { Component, computed, inject, signal } from '@angular/core'
import { MAT_DIALOG_DATA, MatDialogModule, MatDialogRef } from '@angular/material/dialog'
import { MatIconModule } from '@angular/material/icon'
import { connectionKey } from '../../providers/connection'
import { getProviderAdapter } from '../../providers/provider-adapter'
import { ActivityService } from '../../services/activity.service'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SettingsDialogMobileTabsComponent } from './mobile-tabs/settings-dialog-mobile-tabs.component'
import { SettingsDialogNavComponent } from './nav/settings-dialog-nav.component'
import { SettingsGeneralPaneComponent } from './general-pane/settings-general-pane.component'
import { SettingsLogsPaneComponent } from './logs-pane/settings-logs-pane.component'
import { SettingsAddSourcePaneComponent } from './add-source-pane/settings-add-source-pane.component'
import { SettingsSourcePaneComponent } from './source-pane/settings-source-pane.component'
import {
  connectionSelection,
  type SettingsDialogData,
  type SettingsProviderRow,
  type SettingsSelection,
} from './settings-dialog.types'

@Component({
  selector: 'app-settings-dialog',
  imports: [
    MatDialogModule,
    MatIconModule,
    ProviderIconComponent,
    SettingsDialogMobileTabsComponent,
    SettingsDialogNavComponent,
    SettingsGeneralPaneComponent,
    SettingsLogsPaneComponent,
    SettingsAddSourcePaneComponent,
    SettingsSourcePaneComponent,
  ],
  templateUrl: './settings-dialog.component.html',
  styleUrl: './settings-dialog.component.scss',
})
export class SettingsDialogComponent {
  protected readonly activity = inject(ActivityService)
  protected readonly dialogRef = inject(MatDialogRef<SettingsDialogComponent>)

  private readonly data = inject<SettingsDialogData | null>(MAT_DIALOG_DATA, { optional: true })

  protected readonly connectionSelection = connectionSelection
  protected readonly selection = signal<SettingsSelection>(this.data?.initialSelection ?? 'general')
  protected readonly providerRows = computed<SettingsProviderRow[]>(() => {
    return this.activity.providerInfo().map(info => {
      const adapter = getProviderAdapter(info.connection.source)
      return {
        info,
        key: connectionKey(info.connection),
        label: info.label,
        kindLabel: adapter.label,
        tooltip: adapter.getStatusTooltip(info),
      }
    })
  })
  protected readonly selectedProvider = computed<SettingsProviderRow | null>(() => {
    const selection = this.selection()
    if (selection === 'general' || selection === 'logs' || selection === 'add') {
      return null
    }

    return this.providerRows().find(provider => selection === `connection:${provider.key}`) ?? null
  })

  protected close(): void {
    this.dialogRef.close()
  }
}
