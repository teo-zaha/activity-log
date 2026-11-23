import { Component, computed, inject, input, output } from '@angular/core'
import type { ShortcutId } from '../../models/shortcuts'
import { ShortcutService } from '../../services/shortcut.service'
import { IconButtonComponent } from '../button/icon-button.component'
import { KeyTagComponent } from '../shortcut/key-tag.component'

/**
 * The top of a page that covers the view, such as the event details or a settings section: an
 * optional back arrow, the title (the content), actions (elements marked `pageAction`) and an
 * optional close button. A button shows when its label is set.
 */
@Component({
  selector: 'app-page-header',
  imports: [IconButtonComponent, KeyTagComponent],
  template: `
    @if (backLabel(); as label) {
      <button
        appIconButton
        icon="arrow_back"
        class="back"
        [label]="label"
        [attr.aria-keyshortcuts]="backKeys()"
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
        [attr.aria-keyshortcuts]="closeKeys()"
        (click)="dismiss.emit()"
      >
        @if (closeShortcut(); as id) {
          <app-key-tag [shortcut]="id" />
        }
      </button>
    }
  `,
  styleUrl: './page-header.component.scss',
})
export class PageHeaderComponent {
  /** Names the back arrow, which shows only when set. */
  readonly backLabel = input<string | null>(null)
  /** Names the close button, which shows only when set. */
  readonly closeLabel = input<string | null>(null)
  /** The back arrow's `aria-keyshortcuts` (`ShortcutService.ariaKeys`). */
  readonly backKeys = input<string | null>(null)
  /** The close button's shortcut, for its key tag and `aria-keyshortcuts`. */
  readonly closeShortcut = input<ShortcutId | null>(null)
  readonly goBack = output<void>()
  readonly dismiss = output<void>()

  private readonly shortcuts = inject(ShortcutService)
  protected readonly closeKeys = computed(() => {
    const id = this.closeShortcut()
    return id === null ? null : this.shortcuts.ariaKeys(id)
  })
}
