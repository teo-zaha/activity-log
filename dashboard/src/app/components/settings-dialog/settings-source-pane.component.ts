import { Component, input } from '@angular/core'
import { MatCardModule } from '@angular/material/card'
import { MatChipsModule } from '@angular/material/chips'
import { MatDividerModule } from '@angular/material/divider'
import { MatIconModule } from '@angular/material/icon'
import { MatRadioModule } from '@angular/material/radio'
import type { SupportedOs } from '../../generated/activity'
import { StatusPillComponent } from '../ui-status/status-pill.component'
import type { SettingsProviderRow } from './settings-dialog.types'

@Component({
  selector: 'app-settings-source-pane',
  standalone: true,
  imports: [
    MatCardModule,
    MatChipsModule,
    MatDividerModule,
    MatIconModule,
    MatRadioModule,
    StatusPillComponent,
  ],
  templateUrl: './settings-source-pane.component.html',
  styleUrl: './settings-source-pane.component.scss',
})
export class SettingsSourcePaneComponent {
  readonly provider = input.required<SettingsProviderRow>()

  protected supportedOsLabel(os: SupportedOs): string {
    switch (os) {
      case 'linux':
        return 'Linux'
      case 'macos':
        return 'macOS'
      case 'windows':
        return 'Windows'
    }
  }

  protected checkedAtLabel(value: string): string {
    return new Date(value).toLocaleString('en-GB', {
      dateStyle: 'medium',
      timeStyle: 'short',
    })
  }
}
