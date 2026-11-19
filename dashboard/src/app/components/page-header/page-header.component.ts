import { Component, input, output } from '@angular/core'
import { IconButtonComponent } from '../button/icon-button.component'

/**
 * The top of a page that covers the view, such as the event details or a settings section: an
 * optional back arrow, the title (the content), actions (elements marked `pageAction`) and an
 * optional close button. A button shows when its label is set.
 */
@Component({
  selector: 'app-page-header',
  imports: [IconButtonComponent],
  template: `
    @if (backLabel(); as label) {
      <button
        appIconButton
        icon="arrow_back"
        class="back"
        [label]="label"
        (click)="goBack.emit()"
      ></button>
    }
    <div class="title"><ng-content /></div>
    <ng-content select="[pageAction]" />
    @if (closeLabel(); as label) {
      <button
        appIconButton
        icon="close"
        class="close"
        [label]="label"
        (click)="dismiss.emit()"
      ></button>
    }
  `,
  styleUrl: './page-header.component.scss',
})
export class PageHeaderComponent {
  /** Names the back arrow, which shows only when set. */
  readonly backLabel = input<string | null>(null)
  /** Names the close button, which shows only when set. */
  readonly closeLabel = input<string | null>(null)
  readonly goBack = output<void>()
  readonly dismiss = output<void>()
}
