import { Component, ElementRef, inject, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import type { ButtonSize, ButtonType, ButtonVariant } from './button.types'

/**
 * A square button that shows only an icon, so its `label` is required and becomes the
 * accessible name: `<button appIconButton icon="close" label="Close"></button>`. Add `matTooltip`
 * on the element when sighted users need the label too.
 */
@Component({
  selector: 'button[appIconButton], a[appIconButton]',
  imports: [MatIconModule],
  template: `<mat-icon [class.u-spin]="busy()">{{ busy() ? 'autorenew' : icon() }}</mat-icon>`,
  styleUrl: './button.scss',
  host: {
    'data-icon-only': '',
    '[attr.data-variant]': 'variant()',
    '[attr.data-size]': 'size()',
    '[attr.type]': 'isButton ? type() : null',
    '[attr.aria-label]': 'label()',
    '[attr.aria-busy]': 'busy() || null',
  },
})
export class IconButtonComponent {
  readonly icon = input.required<string>()
  readonly label = input.required<string>()
  readonly variant = input<ButtonVariant>('ghost')
  readonly size = input<ButtonSize>('md')
  readonly busy = input(false)
  readonly type = input<ButtonType>('button')

  protected readonly isButton =
    inject<ElementRef<HTMLElement>>(ElementRef).nativeElement instanceof HTMLButtonElement
}
