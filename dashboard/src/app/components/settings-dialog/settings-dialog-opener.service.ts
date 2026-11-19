import { inject, Service } from '@angular/core'
import { MatDialog, type MatDialogRef } from '@angular/material/dialog'
import { SettingsDialogComponent } from './settings-dialog.component'

/** Lives in its own file so the dialog and its panes stay out of the initial bundle. */
@Service()
export class SettingsDialogOpener {
  private readonly dialog = inject(MatDialog)

  /** Opens the dialog on the section the URL names; a phone uses the page host. */
  open(): MatDialogRef<SettingsDialogComponent> {
    return this.dialog.open(SettingsDialogComponent, {
      panelClass: 'settings-dialog-panel',
      backdropClass: 'settings-dialog-backdrop',
      width: 'min(1040px, calc(100vw - 32px))',
      maxWidth: 'calc(100vw - 32px)',
      maxHeight: 'var(--dialog-max-height)',
      autoFocus: 'dialog',
      restoreFocus: true,
      // The URL opens and closes it: going back within the settings must not close it.
      closeOnNavigation: false,
      ariaLabel: 'Settings',
    })
  }
}
