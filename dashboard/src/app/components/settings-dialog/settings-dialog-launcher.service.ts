import {
  Injector,
  Service,
  type Type,
  inject,
  injectAsync,
  runInInjectionContext,
} from '@angular/core'
import { ActivityService } from '../../services/activity.service'
import type { SettingsDialogOpener } from './settings-dialog-opener.service'
import type { SettingsTarget } from './settings-dialog.types'

@Service()
export class SettingsDialogLauncher {
  private readonly injector = inject(Injector)
  private readonly activity = inject(ActivityService)

  private loadOpener = this.createOpenerLoader()
  private loading = false

  /** Opens on a connection, on "Add source", or on General. */
  open(target?: SettingsTarget): void {
    void this.openWhenLoaded(target)
  }

  /** Clicks while the dialog code is still loading are ignored rather than queued. */
  private async openWhenLoaded(target?: SettingsTarget): Promise<void> {
    if (this.loading) return
    this.loading = true

    let opener: SettingsDialogOpener
    try {
      opener = await this.loadOpener()
    } catch {
      // `injectAsync` caches a failed load, so start over for the next attempt.
      this.loadOpener = runInInjectionContext(this.injector, () => this.createOpenerLoader())
      this.activity.error.set('Settings could not be loaded. Try again.')
      return
    } finally {
      this.loading = false
    }

    opener.open(target)
  }

  /** The one place that imports the dialog code, so it stays out of the initial bundle. */
  protected importOpener(): Promise<Type<SettingsDialogOpener>> {
    return import('./settings-dialog-opener.service').then(module => module.SettingsDialogOpener)
  }

  private createOpenerLoader(): () => Promise<SettingsDialogOpener> {
    return injectAsync(() => this.importOpener())
  }
}
