import { A11yModule } from '@angular/cdk/a11y'
import { Component, ElementRef, effect, inject, viewChild } from '@angular/core'
import { TopLayer } from '../../layouts/shell/top-layer.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { ShortcutService } from '../../services/shortcut.service'
import { SettingsDialogLauncher } from './settings-dialog-launcher.service'
import { SettingsDialogComponent } from './settings-dialog.component'

/**
 * The settings on a phone: the last tab of the bottom bar, shown in the main content in place of
 * the view, under the same header. Its pages are the list of sections, then one section. Wider
 * windows use the dialog. Lives in the shell, outside the views' layout, and loads its code while
 * the app is idle. Event details opened over it cover it: the shell makes it `inert` under them,
 * and Escape closes them first (`TopLayer`).
 */
@Component({
  selector: 'app-settings-page-host',
  imports: [A11yModule, SettingsDialogComponent],
  template: `
    @defer (when open(); prefetch on idle) {
      @if (open()) {
        <div
          class="settings-page"
          role="region"
          aria-label="Settings"
          tabindex="-1"
          [cdkTrapFocus]="false"
          [cdkTrapFocusAutoCapture]="true"
        >
          <app-settings-dialog #page chrome="page" />
        </div>
      }
    }
  `,
  styleUrl: './settings-page-host.component.scss',
})
export class SettingsPageHostComponent {
  private readonly viewState = inject(ActivityViewStateService)
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
  protected readonly open = inject(SettingsDialogLauncher).pageOpen
  private readonly layers = inject(TopLayer)
  private readonly shortcuts = inject(ShortcutService)
  // By its reference, not its class: a class query would load the deferred settings at startup.
  private readonly page = viewChild<SettingsDialogComponent>('page')
  private previouslyFocused: HTMLElement | null = null

  constructor() {
    let wasOpen = false
    effect(() => {
      const open = this.open()
      if (open && !wasOpen) {
        const active = document.activeElement
        this.previouslyFocused = active instanceof HTMLElement ? active : null
      }
      if (!open && wasOpen) {
        const element = this.previouslyFocused
        this.previouslyFocused = null
        if (element?.isConnected) queueMicrotask(() => element.focus())
      }
      wasOpen = open
    })

    // While open, the page is a layer that hides the view: Escape closes it while it is on top
    // (details over it close first; a dialog over it, a confirmation or a select's list, handles
    // Escape first, too), and Back leaves its section, then the page.
    effect(onCleanup => {
      if (!this.open()) return
      const onTop = (): boolean => this.layers.top() === 'settingsPage'
      const off = [
        this.shortcuts.openLayer({ kind: 'page', hidesRoute: true }),
        this.shortcuts.bind('layer.close', () => void this.viewState.closeSettings(), onTop),
        this.shortcuts.bind('app.back', () => this.pageBack(), onTop),
      ]
      onCleanup(() => off.forEach(close => close()))
    })

    // Details over the page closed: the page takes the focus they had.
    this.layers.whenExposed('settingsPage', () => {
      const active = document.activeElement
      if (!this.host.contains(active)) this.page()?.focusTitle()
    })
  }

  /** Back on the page, once its code is in; until then the key is left to the app. */
  private pageBack(): false | void {
    const page = this.page()
    if (page === undefined) return false
    page.pageBack()
  }
}
