import { Component, input, output, ChangeDetectionStrategy } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import type { Source } from '../../generated/activity'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { providerStatusLabel } from '../ui-status/provider-status-label'
import { StatusDotComponent } from '../ui-status/status-dot.component'
import type {
  SettingsFutureProviderRow,
  SettingsProviderRow,
  SettingsSelection,
} from './settings-dialog.types'

@Component({
  selector: 'app-settings-dialog-nav',
  imports: [MatIconModule, MatTooltipModule, ProviderIconComponent, StatusDotComponent],
  templateUrl: './settings-dialog-nav.component.html',
  styleUrl: './settings-dialog-nav.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SettingsDialogNavComponent {
  readonly selection = input.required<SettingsSelection>()
  readonly providers = input.required<readonly SettingsProviderRow[]>()
  readonly futureProviders = input.required<readonly SettingsFutureProviderRow[]>()

  readonly generalSelected = output<void>()
  readonly sourceSelected = output<Source>()

  protected selectGeneral(): void {
    this.generalSelected.emit()
  }

  protected selectSource(source: Source): void {
    this.sourceSelected.emit(source)
  }

  protected isSourceSelected(source: Source): boolean {
    return this.selection() === `source:${source}`
  }
  protected readonly providerStatusLabel = providerStatusLabel
}
