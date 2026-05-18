import { Component, inject, ChangeDetectionStrategy } from '@angular/core'
import { MatCardModule } from '@angular/material/card'
import { MatIconModule } from '@angular/material/icon'
import { THEMES, ThemeService, type Theme } from '../../services/theme.service'

@Component({
  selector: 'app-settings-theme-card',
  imports: [MatCardModule, MatIconModule],
  templateUrl: './settings-theme-card.component.html',
  styleUrl: './settings-theme-card.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SettingsThemeCardComponent {
  private readonly theme = inject(ThemeService)
  protected readonly themes = THEMES

  protected isThemeActive(themeId: Theme): boolean {
    return this.theme.theme() === themeId
  }

  protected selectTheme(themeId: Theme): void {
    this.theme.setTheme(themeId)
  }
}
