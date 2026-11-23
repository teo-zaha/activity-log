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
import { ActivityDataService } from './services/activity-data.service'
import { ActivityViewStateService } from './services/activity-view-state.service'
import { NativeMenuService } from './services/native-menu/native-menu.service'
import type { NoticeAction } from './services/failure'
import { LegacyImportNoticeService } from './services/legacy-import-notice.service'
import { NoticeService } from './services/notice.service'
import { ThemeService } from './services/theme.service'
import { followTouchInput } from './utils/touch-input'
import { WindowChromeService } from './services/window-chrome.service'

/** What a notice's button says for what it does. */
const ACTION_LABEL: Record<NoticeAction['kind'], string> = {
  openSettings: 'Open Settings',
  importLegacy: 'Import',
}

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
  protected readonly data = inject(ActivityDataService)
  protected readonly viewState = inject(ActivityViewStateService)
  private readonly notices = inject(NoticeService)
  private readonly legacyImport = inject(LegacyImportNoticeService)
  protected readonly chrome = inject(WindowChromeService)
  protected readonly theme = inject(ThemeService)
  private readonly document = inject(DOCUMENT)
  private readonly snackBar = inject(MatSnackBar)
  private snackRef: MatSnackBarRef<TextOnlySnackBar> | null = null

  constructor() {
    inject(SettingsDialogLauncher).followUrl()
    // The macOS menu bar, in the desktop app there; the page keeps every key until it is in.
    inject(NativeMenuService).start()

    followTouchInput(this.document)

    // Collection failures are not among these: <app-collection-status> shows them with Retry.
    effect(() => {
      const notice = this.notices.current()
      if (notice && 'failure' in notice && notice.failure.cause !== undefined) {
        console.warn(notice.failure.message, notice.failure.cause)
      }
      this.snackRef?.dismiss()
      this.snackRef = null
      if (!notice) return
      const failed = 'failure' in notice
      const action = notice.action
      this.snackRef = this.snackBar.open(
        failed ? notice.failure.message : notice.info,
        action ? ACTION_LABEL[action.kind] : 'Dismiss',
        {
          horizontalPosition: 'center',
          verticalPosition: 'bottom',
          panelClass: [failed ? 'error-alert-snackbar' : 'info-notice'],
        }
      )
      if (action) {
        this.snackRef.onAction().subscribe(() => {
          this.notices.clear(notice.origin)
          this.run(action)
        })
      }
    })
  }

  private run(action: NoticeAction): void {
    switch (action.kind) {
      case 'openSettings':
        void this.viewState.openSettings(action.selection)
        return
      case 'importLegacy':
        void this.legacyImport.importNow()
        return
    }
  }

  /** The skip link's target is the shell's `main`; the URL stays as it is. */
  protected skipToMain(event: MouseEvent): void {
    event.preventDefault()
    this.document.getElementById('main-content')?.focus()
  }

  ngOnInit(): void {
    void this.data.initLoad()
  }
}
