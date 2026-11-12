import { Component, input, output } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import type { Source } from '../../../generated/activity'
import { ProviderIconComponent } from '../../provider-icon/provider-icon.component'
import { StatusDotComponent } from '../../ui-status/status-dot.component'
import type {
  SettingsFutureProviderRow,
  SettingsProviderRow,
  SettingsSelection,
} from '../settings-dialog.types'

@Component({
  selector: 'app-settings-dialog-mobile-tabs',
  imports: [MatIconModule, ProviderIconComponent, StatusDotComponent],
  templateUrl: './settings-dialog-mobile-tabs.component.html',
  styleUrl: './settings-dialog-mobile-tabs.component.scss',
})
export class SettingsDialogMobileTabsComponent {
  readonly selection = input.required<SettingsSelection>()
  readonly providers = input.required<readonly SettingsProviderRow[]>()
  readonly futureProviders = input.required<readonly SettingsFutureProviderRow[]>()

  readonly generalSelected = output<void>()
  readonly logsSelected = output<void>()
  readonly sourceSelected = output<Source>()

  protected selectGeneral(): void {
    this.generalSelected.emit()
  }

  protected selectLogs(): void {
    this.logsSelected.emit()
  }

  protected selectSource(source: Source): void {
    this.sourceSelected.emit(source)
  }
}
