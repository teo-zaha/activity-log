import { Component, input } from '@angular/core'
import type { StackLevelVm } from '../../providers/change-stack'
import { StackChangeComponent } from './stack-change.component'

/**
 * Beyond this depth the tree above indents no further, so a long chain keeps the width its titles
 * need, on a phone too; the nesting stays in the lists.
 */
export const MAX_STACK_INDENT = 4

/**
 * A level of the tree above a change: the changes that build on it, each with its own level in a
 * nested list, and a last line when the level was cut. Nothing shows for a level that is
 * complete and empty.
 */
@Component({
  selector: 'app-stack-level',
  imports: [StackChangeComponent],
  host: { '[class.is-indented]': 'level().depth <= maxIndent' },
  template: `@let current = level();
    @if (current.nodes.length) {
      <ol class="level">
        @for (node of current.nodes; track $index) {
          <li class="node">
            <app-stack-change [change]="node.change" />
            <app-stack-level [level]="node.above" />
          </li>
        }
      </ol>
    }
    @if (current.end.kind === 'cut') {
      <p class="cut">{{ current.end.text }}</p>
    }`,
  styleUrl: './stack-level.component.scss',
})
export class StackLevelComponent {
  readonly level = input.required<StackLevelVm>()
  protected readonly maxIndent = MAX_STACK_INDENT
}
