import { Component, inject } from '@angular/core'
import { MAT_DIALOG_DATA, MatDialog, MatDialogModule } from '@angular/material/dialog'
import { firstValueFrom } from 'rxjs'

export interface ConfirmDialogData {
  readonly title: string
  readonly message: string
  readonly confirmLabel: string
  /** `danger` for an action that cannot be undone, such as removing something. */
  readonly tone?: 'danger'
}

/** Asks before an action; resolves to whether the user confirmed. */
export async function confirmAction(dialog: MatDialog, data: ConfirmDialogData): Promise<boolean> {
  const ref = dialog.open<ConfirmDialogComponent, ConfirmDialogData, boolean>(
    ConfirmDialogComponent,
    {
      data,
      panelClass: 'app-dialog-panel',
      backdropClass: 'settings-dialog-backdrop',
      width: 'min(420px, calc(100vw - 32px))',
      autoFocus: 'first-tabbable',
      restoreFocus: true,
      role: 'alertdialog',
    }
  )
  return (await firstValueFrom(ref.afterClosed())) === true
}

@Component({
  selector: 'app-confirm-dialog',
  imports: [MatDialogModule],
  templateUrl: './confirm-dialog.component.html',
  styleUrl: './confirm-dialog.component.scss',
})
export class ConfirmDialogComponent {
  protected readonly data = inject<ConfirmDialogData>(MAT_DIALOG_DATA)
}
