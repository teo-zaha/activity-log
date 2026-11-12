import { inject, Service } from '@angular/core'
import { MatDialog } from '@angular/material/dialog'
import type { Source } from '../../generated/activity'
import { BREAKPOINTS } from '../../utils/breakpoints'
import { SettingsDialogComponent } from './settings-dialog.component'
import type { SettingsDialogData } from './settings-dialog.types'

/** Lives in its own file so the dialog and its panes stay out of the initial bundle. */
@Service()
export class SettingsDialogOpener {
  private readonly dialog = inject(MatDialog)

  open(source?: Source): void {
    const fullscreen = window.matchMedia?.(`(max-width: ${BREAKPOINTS.phone}px)`).matches ?? false
    const data: SettingsDialogData = {
      initialSelection: source ? `source:${source}` : 'general',
    }

    this.dialog.open(SettingsDialogComponent, {
      data,
      panelClass: fullscreen
        ? ['settings-dialog-panel', 'settings-dialog-fullscreen']
        : 'settings-dialog-panel',
      backdropClass: 'settings-dialog-backdrop',
      width: fullscreen ? '100vw' : 'min(1040px, calc(100vw - 32px))',
      maxWidth: fullscreen ? '100vw' : 'calc(100vw - 32px)',
      height: fullscreen ? '100dvh' : undefined,
      maxHeight: fullscreen ? '100dvh' : 'calc(100dvh - 32px)',
      autoFocus: 'dialog',
      restoreFocus: true,
      ariaLabel: 'Settings',
    })
  }
}
