import { DOCUMENT } from '@angular/common'
import { Component, OnInit, effect, inject } from '@angular/core'
import { MatProgressBarModule } from '@angular/material/progress-bar'
import {
  MatSnackBar,
  MatSnackBarModule,
  type MatSnackBarRef,
  type TextOnlySnackBar,
} from '@angular/material/snack-bar'
import { RouterOutlet } from '@angular/router'
import { SettingsDialogLauncher } from './components/settings-dialog/settings-dialog-launcher.service'
import { WindowControlsComponent } from './components/window-controls/window-controls.component'
import { ActivityService } from './services/activity.service'
import { ThemeService } from './services/theme.service'
import { WindowChromeService } from './services/window-chrome.service'

/**
 * What belongs to the app rather than to a page: the skip link, the loading bar, the notices, the
 * window buttons on Windows, and the routed shell (`ShellLayoutComponent`).
 */
@Component({
  selector: 'app-root',
  imports: [MatProgressBarModule, MatSnackBarModule, RouterOutlet, WindowControlsComponent],
  templateUrl: './app.component.html',
  styleUrl: './app.component.scss',
})
export class AppComponent implements OnInit {
  protected readonly activity = inject(ActivityService)
  protected readonly chrome = inject(WindowChromeService)
  protected readonly theme = inject(ThemeService)
  private readonly document = inject(DOCUMENT)
  private readonly snackBar = inject(MatSnackBar)
  private snackRef: MatSnackBarRef<TextOnlySnackBar> | null = null

  constructor() {
    inject(SettingsDialogLauncher).followUrl()

    // Collection failures are not among these: <app-collection-status> shows them with Retry.
    effect(() => {
      const notice = this.activity.notice()
      if (notice?.failure.cause !== undefined) {
        console.warn(notice.failure.message, notice.failure.cause)
      }
      this.snackRef?.dismiss()
      this.snackRef =
        notice &&
        this.snackBar.open(notice.failure.message, 'Dismiss', {
          horizontalPosition: 'center',
          verticalPosition: 'bottom',
          panelClass: ['error-alert-snackbar'],
        })
    })
  }

  /** The skip link's target is the shell's `main`; the URL stays as it is. */
  protected skipToMain(event: MouseEvent): void {
    event.preventDefault()
    this.document.getElementById('main-content')?.focus()
  }

  ngOnInit(): void {
    void this.activity.initLoad()
  }
}
