import { Component, computed, inject } from '@angular/core'
import { MatDialog, MatDialogModule } from '@angular/material/dialog'
import { SHORTCUT_SCOPES, SHORTCUTS, type ShortcutScope } from '../../models/shortcuts'
import { SHORTCUT_HELP_DIALOG_ID, ShortcutService } from '../../services/shortcut.service'
import { ButtonComponent } from '../button/button.component'

/** Every scope's section title: a scope added to `SHORTCUT_SCOPES` needs one here. */
const SCOPE_TITLES: Readonly<Record<ShortcutScope, string>> = {
  global: 'Everywhere',
  timeline: 'Timeline',
  calendar: 'Calendar',
  details: 'Event details',
}

interface HelpRow {
  readonly label: string
  /** Every chord of the command here, in the table's order: the template joins them with "or". */
  readonly keys: readonly string[]
}

interface HelpSection {
  readonly title: string
  readonly rows: readonly HelpRow[]
}

/** Opens the help, or closes it when it is open: `?` does both. */
export function toggleShortcutHelp(dialogs: MatDialog): void {
  const open = dialogs.getDialogById(SHORTCUT_HELP_DIALOG_ID)
  if (open) {
    open.close()
    return
  }
  dialogs.open(ShortcutHelpDialogComponent, {
    id: SHORTCUT_HELP_DIALOG_ID,
    panelClass: 'app-dialog-panel',
    backdropClass: 'settings-dialog-backdrop',
    width: 'min(720px, var(--dialog-max-width))',
    // Material caps a dialog at 560px otherwise: too narrow for the help's two columns.
    maxWidth: 'var(--dialog-max-width)',
    maxHeight: 'var(--dialog-max-height)',
    autoFocus: 'dialog',
    restoreFocus: true,
    ariaLabelledBy: 'shortcut-help-title',
  })
}

/**
 * The help (`?`): every command that has a key here, once, with all its keys, by where it
 * works, from the registry. With single-key shortcuts off it lists only the keys left.
 */
@Component({
  selector: 'app-shortcut-help-dialog',
  imports: [ButtonComponent, MatDialogModule],
  templateUrl: './shortcut-help-dialog.component.html',
  styleUrl: './shortcut-help-dialog.component.scss',
})
export class ShortcutHelpDialogComponent {
  private readonly shortcuts = inject(ShortcutService)

  protected readonly sections = computed<readonly HelpSection[]>(() =>
    SHORTCUT_SCOPES.map(scope => ({
      title: SCOPE_TITLES[scope],
      rows: SHORTCUTS.filter(entry => entry.scope === scope).flatMap(entry => {
        const keys = this.shortcuts.chords(entry).map(chord => this.shortcuts.format(chord))
        return keys.length > 0 ? [{ label: entry.label, keys }] : []
      }),
    })).filter(section => section.rows.length > 0)
  )
}
