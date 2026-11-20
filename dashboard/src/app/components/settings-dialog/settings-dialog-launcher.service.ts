import {
  Injector,
  Service,
  type Type,
  computed,
  effect,
  inject,
  injectAsync,
  runInInjectionContext,
  signal,
  untracked,
} from '@angular/core'
import type { MatDialogRef } from '@angular/material/dialog'
import { mediaQuery } from '@signality/core/browser/media-query'
import { ActivityService } from '../../services/activity.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { failureWhile } from '../../services/failure'
import { downQuery } from '../../utils/breakpoints'
import type { SettingsDialogOpener } from './settings-dialog-opener.service'
import { type SettingsSelection, type SettingsTarget } from './settings-dialog.types'

/**
 * The settings are a URL state: `?settings=<section>` shows them over the view. Opening them
 * navigates there, and the settings follow the URL, so Back closes them and a link opens them.
 *
 * They show in a dialog, and on a phone as the bottom bar's Settings tab, in the main content in
 * place of the view (`SettingsPageHostComponent`). A window resized across the phone width moves
 * them between the two; the section is in the URL and unsaved edits in `SourceDrafts`, so nothing
 * is lost.
 */
@Service()
export class SettingsDialogLauncher {
  private readonly injector = inject(Injector)
  private readonly activity = inject(ActivityService)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly requested = computed(() => this.viewState.settings() !== null)
  private readonly phone = mediaQuery(downQuery('phone'))
  private readonly host = signal<'dialog' | 'page' | null>(null)
  /** Whether the settings show as the phone's Settings tab rather than in the dialog. */
  readonly pageOpen = computed(() => this.host() === 'page')
  /** Whether the settings show in the dialog, or will once its code has loaded. */
  readonly dialogOpen = computed(() => this.host() === 'dialog')

  private loadOpener = this.createOpenerLoader()
  private loading = false
  private dialog: MatDialogRef<unknown> | null = null
  private following = false

  /** Opens on a connection, on "Add source", or on the start: the list on a phone, else General. */
  open(target?: SettingsTarget): void {
    void this.viewState.openSettings(selectionFor(target, this.phone()))
  }

  /** Opens and closes the dialog as the URL asks for it, from now on. */
  followUrl(): void {
    if (this.following) return
    this.following = true
    effect(
      () => {
        const requested = this.requested()
        const host = this.phone() ? 'page' : 'dialog'
        untracked(() => (requested ? this.showIn(host) : this.hide()))
      },
      { injector: this.injector }
    )
  }

  private showIn(host: 'dialog' | 'page'): void {
    this.host.set(host)
    if (host === 'dialog') void this.show()
    else this.closeDialog()
  }

  private hide(): void {
    // Also while the dialog code still loads, so it does not open once loaded.
    this.host.set(null)
    this.closeDialog()
  }

  /** Closes the dialog without leaving the settings URL: the page takes over, or the URL left. */
  private closeDialog(): void {
    const dialog = this.dialog
    this.dialog = null
    dialog?.close()
  }

  /** A URL change while the dialog code is still loading is picked up once it has loaded. */
  private async show(): Promise<void> {
    if (this.loading || this.dialog) return
    this.loading = true

    let opener: SettingsDialogOpener
    try {
      opener = await this.loadOpener()
    } catch (error) {
      // `injectAsync` caches a failed load, so start over for the next attempt.
      this.loadOpener = runInInjectionContext(this.injector, () => this.createOpenerLoader())
      // Only a dialog still wanted failed: the phone page shows without this code, and settings
      // the URL left meanwhile are closed already.
      if (this.host() !== 'dialog' || !this.requested()) return
      this.activity.notice.set({
        origin: 'settings',
        failure: failureWhile('Loading the settings', error),
      })
      this.host.set(null)
      void this.viewState.closeSettings()
      return
    } finally {
      this.loading = false
    }

    // The URL may have dropped the settings meanwhile, before the effect that hides them ran.
    if (!this.requested()) this.host.set(null)
    if (this.host() !== 'dialog' || this.dialog) return
    const dialog = opener.open()
    this.dialog = dialog
    dialog.afterClosed().subscribe(() => {
      if (this.dialog !== dialog) return
      this.dialog = null
      this.host.set(null)
      // Closed in the dialog rather than by the URL.
      if (this.requested()) void this.viewState.closeSettings()
    })
  }

  /** The one place that imports the dialog code, so it stays out of the initial bundle. */
  protected importOpener(): Promise<Type<SettingsDialogOpener>> {
    return import('./settings-dialog-opener.service').then(module => module.SettingsDialogOpener)
  }

  private createOpenerLoader(): () => Promise<SettingsDialogOpener> {
    return injectAsync(() => this.importOpener())
  }
}

function selectionFor(target: SettingsTarget | undefined, phone: boolean): SettingsSelection {
  if (target === undefined) return phone ? 'home' : 'general'
  return target
}
