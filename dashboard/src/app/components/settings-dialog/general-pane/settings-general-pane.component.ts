import { Component } from '@angular/core'
import { SettingsKeyboardCardComponent } from './keyboard-card/settings-keyboard-card.component'
import { SettingsThemeCardComponent } from './theme-card/settings-theme-card.component'

/** Settings › General: the theme, and the keyboard. Each applies at once and is kept on the device. */
@Component({
  selector: 'app-settings-general-pane',
  imports: [SettingsKeyboardCardComponent, SettingsThemeCardComponent],
  templateUrl: './settings-general-pane.component.html',
  styleUrl: './settings-general-pane.component.scss',
})
export class SettingsGeneralPaneComponent {}
