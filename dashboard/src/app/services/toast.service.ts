import { inject, Service } from '@angular/core'
import { MatSnackBar } from '@angular/material/snack-bar'

/** Short confirmations that an action worked, such as a save; errors stay next to their cause. */
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
}
