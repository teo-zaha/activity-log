import { Component, input } from '@angular/core'
import type { ChangeStackVm } from '../../providers/change-stack'
import { StackChangeComponent } from './stack-change.component'
import { StackLevelComponent } from './stack-level.component'

/** The levels the section's heading can take, under the page's own `h1` and a panel's `h2`. */
export type StackHeadingLevel = 2 | 3 | 4

let nextId = 0

/**
 * The Stack section: where a change stands among its repository's open changes, from what the
 * stack stands on up to the changes built on it. The backend computes the stack (`ChangeStack`);
 * a host's adapter lays it out (`changeStackVmOf`). The caller says how fresh it is and where the
 * section sits in its page's headings. Both lists keep `role="list"`: WebKit presents a list
 * without markers as no list, and VoiceOver would read the stack flat.
 */
@Component({
  selector: 'app-change-stack',
  imports: [StackChangeComponent, StackLevelComponent],
  templateUrl: './change-stack.component.html',
  styleUrl: './change-stack.component.scss',
})
export class ChangeStackComponent {
  readonly stack = input.required<ChangeStackVm>()
  /** How fresh the stack is: the details show the one the last collection found. */
  readonly caption = input('As of the last collection')
  readonly headingLevel = input<StackHeadingLevel>(3)
  protected readonly headingId = `change-stack-title-${nextId++}`
}
