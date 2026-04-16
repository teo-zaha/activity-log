import { Component } from '@angular/core'
import { SettingsThemeCardComponent } from './settings-theme-card.component'

@Component({
  selector: 'app-settings-general-pane',
  standalone: true,
  imports: [SettingsThemeCardComponent],
  templateUrl: './settings-general-pane.component.html',
  styleUrl: './settings-general-pane.component.scss',
})
export class SettingsGeneralPaneComponent {
}
