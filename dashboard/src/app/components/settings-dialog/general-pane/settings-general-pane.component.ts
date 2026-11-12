import { Component } from '@angular/core'
import { SettingsThemeCardComponent } from './theme-card/settings-theme-card.component'

@Component({
  selector: 'app-settings-general-pane',
  imports: [SettingsThemeCardComponent],
  templateUrl: './settings-general-pane.component.html',
  styleUrl: './settings-general-pane.component.scss',
})
export class SettingsGeneralPaneComponent {}
