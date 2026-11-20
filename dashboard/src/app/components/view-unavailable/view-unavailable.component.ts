import { DOCUMENT } from '@angular/common'
import { Component, inject } from '@angular/core'
import { ButtonComponent } from '../button/button.component'
import { EmptyStateComponent } from '../empty-state/empty-state.component'

/**
 * In place of a view whose code did not load as the app started (an app update, a dev server
 * that went away): the shell and the navigation are there, and the app can be loaded again.
 */
@Component({
  selector: 'app-view-unavailable',
  imports: [ButtonComponent, EmptyStateComponent],
  template: `
    <app-empty-state icon="cloud_off" heading="This view could not be loaded">
      The app may have been updated, or its files are out of reach. The other views may still open
      from the navigation.
      <button appButton emptyStateAction variant="primary" icon="refresh" (click)="reload()">
        Reload the app
      </button>
    </app-empty-state>
  `,
  styles: `
    :host {
      display: flex;
      flex: 1;
      min-height: 0;
      padding: 24px;
      overflow-y: auto;
      overscroll-behavior-y: contain;
    }
  `,
})
export class ViewUnavailableComponent {
  private readonly document = inject(DOCUMENT)

  /** A page reload: Signality has nothing for it, and nothing outlives it. */
  protected reload(): void {
    this.document.defaultView?.location.reload()
  }
}
