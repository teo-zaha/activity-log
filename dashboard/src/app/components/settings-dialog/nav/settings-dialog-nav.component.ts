import { Component, input, output } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { ProviderIconComponent } from '../../provider-icon/provider-icon.component'
import { providerStatusLabel } from '../../ui-status/provider-status-label'
import { StatusDotComponent } from '../../ui-status/status-dot.component'
import {
  SETTINGS_SECTIONS,
  connectionSelection,
  type SettingsProviderRow,
  type SettingsSelection,
} from '../settings-dialog.types'

@Component({
  selector: 'app-settings-dialog-nav',
  imports: [MatIconModule, MatTooltipModule, ProviderIconComponent, StatusDotComponent],
  templateUrl: './settings-dialog-nav.component.html',
  styleUrl: './settings-dialog-nav.component.scss',
})
export class SettingsDialogNavComponent {
  readonly selection = input.required<SettingsSelection>()
  readonly providers = input.required<readonly SettingsProviderRow[]>()
  readonly selected = output<SettingsSelection>()

  protected readonly sections = SETTINGS_SECTIONS
  protected readonly connectionSelection = connectionSelection
  protected readonly providerStatusLabel = providerStatusLabel
}
