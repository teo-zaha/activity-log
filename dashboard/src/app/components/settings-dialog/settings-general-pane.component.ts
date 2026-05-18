import { Component, ChangeDetectionStrategy } from '@angular/core'
import { SettingsThemeCardComponent } from './settings-theme-card.component'

@Component({
  selector: 'app-settings-general-pane',
  imports: [SettingsThemeCardComponent],
  templateUrl: './settings-general-pane.component.html',
  styleUrl: './settings-general-pane.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SettingsGeneralPaneComponent {}
