import { Component, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'

/**
 * Explains why there is nothing to show. The content is the message; an element marked
 * `emptyStateAction` goes below it, and one marked `emptyStateVisual` replaces the icon.
 */
@Component({
  selector: 'app-empty-state',
  imports: [MatIconModule],
  templateUrl: './empty-state.component.html',
  styleUrl: './empty-state.component.scss',
  host: { '[attr.data-appearance]': 'appearance()' },
})
export class EmptyStateComponent {
  readonly heading = input.required<string>()
  readonly icon = input<string>()
  readonly level = input<2 | 3>(3)
  /** `panel` is a dashed panel of its own; `plain` sits inside another surface, such as a list. */
  readonly appearance = input<'panel' | 'plain'>('panel')
}
