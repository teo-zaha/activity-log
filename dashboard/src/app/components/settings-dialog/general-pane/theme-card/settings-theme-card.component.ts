import { Component, ElementRef, inject, viewChildren } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { THEMES, ThemeService, type Theme } from '../../../../services/theme.service'
import { radioKeyTarget } from '../../../segmented-control/segmented-control.component'

/** The themes as swatches that preview themselves; one choice, a radio group. */
@Component({
  selector: 'app-settings-theme-card',
  imports: [MatIconModule],
  templateUrl: './settings-theme-card.component.html',
  styleUrl: './settings-theme-card.component.scss',
  host: {
    role: 'radiogroup',
    'aria-label': 'Theme',
    '(keydown)': 'onKeydown($event)',
  },
})
export class SettingsThemeCardComponent {
  private readonly theme = inject(ThemeService)
  protected readonly themes = THEMES
  private readonly swatches = viewChildren<ElementRef<HTMLButtonElement>>('swatch')

  protected isThemeActive(themeId: Theme): boolean {
    return this.theme.theme() === themeId
  }

  protected selectTheme(themeId: Theme): void {
    this.theme.setTheme(themeId)
  }

  protected onKeydown(event: KeyboardEvent): void {
    const current = this.themes.findIndex(option => this.isThemeActive(option.id))
    const next = radioKeyTarget(event.key, current, this.themes.length)
    const option = next === null ? undefined : this.themes[next]
    if (next === null || !option) return
    event.preventDefault()
    this.selectTheme(option.id)
    this.swatches()[next]?.nativeElement.focus()
  }
}
