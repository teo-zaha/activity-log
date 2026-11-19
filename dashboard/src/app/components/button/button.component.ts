import { Component, ElementRef, inject, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import type { ButtonSize, ButtonType, ButtonVariant } from './button.types'

/**
 * The app's button, on a native `<button>` or `<a>`: `<button appButton variant="primary"
 * icon="sync">Collect</button>`. A `<button>` defaults to `type="button"`.
 */
@Component({
  selector: 'button[appButton], a[appButton]',
  imports: [MatIconModule],
  template: `
    @if (busy()) {
      <mat-icon class="u-spin">autorenew</mat-icon>
    } @else if (icon(); as icon) {
      <mat-icon>{{ icon }}</mat-icon>
    }
    <ng-content />
  `,
  styleUrl: './button.scss',
  host: {
    '[attr.data-variant]': 'variant()',
    '[attr.data-size]': 'size()',
    '[attr.type]': 'isButton ? type() : null',
    '[attr.aria-busy]': 'busy() || null',
  },
})
export class ButtonComponent {
  readonly variant = input<ButtonVariant>('secondary')
  readonly size = input<ButtonSize>('md')
  /** A Material Icons ligature shown before the label. */
  readonly icon = input<string>()
  /** Marks the button busy and spins an icon; it stays enabled unless `disabled` is bound too. */
  readonly busy = input(false)
  readonly type = input<ButtonType>('button')

  protected readonly isButton =
    inject<ElementRef<HTMLElement>>(ElementRef).nativeElement instanceof HTMLButtonElement
}
