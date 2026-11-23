import { Component, inject } from '@angular/core'
import { MatDialog } from '@angular/material/dialog'
import { MatSlideToggleModule } from '@angular/material/slide-toggle'
import { BreakpointState } from '../../../../services/breakpoint-state.service'
import { ShortcutPreferences } from '../../../../services/shortcut-preferences.service'
import { ButtonComponent } from '../../../button/button.component'
import { KeyTagComponent } from '../../../shortcut/key-tag.component'
import { openShortcutHelp } from '../../../shortcut/shortcut-help-dialog.component'

/**
 * The keyboard settings: the switch for single-key shortcuts (WCAG 2.1.4), and in the phone layout
 * the way to the help without `?`, which the sidebar is elsewhere. Shown on touch screens too: a
 * phone or tablet may have a keyboard, and whoever can type letters must be able to turn letter
 * keys off.
 */
@Component({
  selector: 'app-settings-keyboard-card',
  imports: [ButtonComponent, KeyTagComponent, MatSlideToggleModule],
  templateUrl: './settings-keyboard-card.component.html',
  styleUrl: './settings-keyboard-card.component.scss',
})
export class SettingsKeyboardCardComponent {
  private readonly preferences = inject(ShortcutPreferences)
  private readonly dialogs = inject(MatDialog)
  protected readonly characterKeys = this.preferences.characterKeys
  /** A phone has no sidebar, where the help opens from on wider screens. */
  protected readonly isPhone = inject(BreakpointState).phone

  protected setCharacterKeys(on: boolean): void {
    this.preferences.setCharacterKeys(on ? 'on' : 'off')
  }

  protected openHelp(): void {
    openShortcutHelp(this.dialogs)
  }
}
