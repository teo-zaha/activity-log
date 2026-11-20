import { inject, Service } from '@angular/core'
import { MatSnackBar } from '@angular/material/snack-bar'
import type { Failure } from './failure'

/**
 * Short confirmations that an action worked, such as a save. Errors stay next to their cause;
 * `failure` is for an action that has none.
 */
@Service()
export class ToastService {
  private readonly snackBar = inject(MatSnackBar)

  success(message: string): void {
    this.snackBar.open(message, undefined, {
      duration: 2500,
      horizontalPosition: 'center',
      verticalPosition: 'bottom',
      panelClass: ['success-toast'],
      politeness: 'polite',
    })
  }

  /** An action that has no place of its own to show why it did not work, such as a copy. */
  failure(failure: Failure): void {
    if (failure.cause !== undefined) console.warn(failure.message, failure.cause)
    this.snackBar.open(failure.message, undefined, {
      duration: 4000,
      horizontalPosition: 'center',
      verticalPosition: 'bottom',
      panelClass: ['error-alert-snackbar'],
      politeness: 'assertive',
    })
  }
}
