import { A11yModule } from '@angular/cdk/a11y'
import { Component, effect, inject } from '@angular/core'
import { ActivityService } from '../../services/activity.service'
import { SettingsDialogLauncher } from './settings-dialog-launcher.service'
import { SettingsDialogComponent } from './settings-dialog.component'

/**
 * The settings on a phone: the last tab of the bottom bar, shown in the main content in place of
 * the view, under the same header. Its pages are the list of sections, then one section. Wider
 * windows use the dialog. Lives in the app shell, so a view transition finds the page in the next
 * frame, and loads its code while the app is idle.
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
          <app-settings-dialog chrome="page" />
        </div>
      }
    }
  `,
  styleUrl: './settings-page-host.component.scss',
  host: {
    // On the document, so Escape closes even when focus is on nothing.
    '(document:keydown.escape)': 'onEscape($event)',
  },
})
export class SettingsPageHostComponent {
  private readonly activity = inject(ActivityService)
  protected readonly open = inject(SettingsDialogLauncher).pageOpen
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
  }

  /** A dialog over the page (a confirmation, a select's list) handles Escape first. */
  protected onEscape(event: Event): void {
    if (!this.open() || event.defaultPrevented) return
    event.preventDefault()
    void this.activity.closeSettings()
  }
}
