import { Component, input, output } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { ProviderIconComponent } from '../../provider-icon/provider-icon.component'
import { StatusDotComponent } from '../../ui-status/status-dot.component'
import {
  SETTINGS_SECTIONS,
  connectionSelection,
  type SettingsProviderRow,
  type SettingsSelection,
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
  readonly selected = output<SettingsSelection>()

  protected readonly sections = SETTINGS_SECTIONS
  protected readonly connectionSelection = connectionSelection
}
