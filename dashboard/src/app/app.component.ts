import { Component, OnInit, afterNextRender, effect, inject } from '@angular/core'
import { MatProgressBarModule } from '@angular/material/progress-bar'
import {
  MatSnackBar,
  MatSnackBarModule,
  type MatSnackBarRef,
  type TextOnlySnackBar,
} from '@angular/material/snack-bar'
import { RouterOutlet } from '@angular/router'
import { ActivityService } from './services/activity.service'
import { ThemeService } from './services/theme.service'
import { HeaderComponent } from './components/header/header.component'
import { SidebarComponent } from './components/sidebar/sidebar.component'
import { SettingsDialogLauncher } from './components/settings-dialog/settings-dialog-launcher.service'
import { EventDetailComponent } from './components/event-detail/event-detail.component'
import { SettingsPageHostComponent } from './components/settings-dialog/settings-page-host.component'
import { CollectionStatusComponent } from './components/collection-status/collection-status.component'
import { NoSourcesComponent } from './components/no-sources/no-sources.component'
import { dismissSplash } from './utils/splash'

import { WindowControlsComponent } from './components/window-controls/window-controls.component'
import { WindowChromeService } from './services/window-chrome.service'
@Component({
  selector: 'app-root',
  imports: [
    MatProgressBarModule,
    MatSnackBarModule,
    RouterOutlet,
    HeaderComponent,
    SidebarComponent,
    EventDetailComponent,
    SettingsPageHostComponent,
    CollectionStatusComponent,
    NoSourcesComponent,
    WindowControlsComponent,
  ],
  templateUrl: './app.component.html',
  styleUrl: './app.component.scss',
})
export class AppComponent implements OnInit {
  protected readonly activity = inject(ActivityService)
  protected readonly chrome = inject(WindowChromeService)
  protected readonly theme = inject(ThemeService)
  private readonly settings = inject(SettingsDialogLauncher)
  /** On a phone the settings are a tab: they take the view's place in the main content. */
  protected readonly settingsTab = this.settings.pageOpen
  private readonly snackBar = inject(MatSnackBar)
  private snackRef: MatSnackBarRef<TextOnlySnackBar> | null = null
  private activeErrorMessage: string | null = null

  constructor() {
    this.settings.followUrl()
    // The shell is in the DOM now; the data streams in later and does not hold the splash.
    afterNextRender(() => dismissSplash())

    effect(() => {
      const error = this.activity.error()
      const status = this.activity.collectionStatus()
      // Collection failures are shown inline by <app-collection-status> with a retry action.
      const shownInline =
        (status.kind === 'timedOut' || status.kind === 'failed') && status.message === error

      if (!error || shownInline) {
        this.snackRef?.dismiss()
        this.snackRef = null
        this.activeErrorMessage = null
        return
      }

      if (this.activeErrorMessage === error) {
        return
      }

      this.snackRef?.dismiss()
      this.activeErrorMessage = error
      this.snackRef = this.snackBar.open(error, 'Dismiss', {
        horizontalPosition: 'center',
        verticalPosition: 'bottom',
        panelClass: ['error-alert-snackbar'],
      })
    })
  }

  ngOnInit(): void {
    void this.activity.initLoad()
  }
}
