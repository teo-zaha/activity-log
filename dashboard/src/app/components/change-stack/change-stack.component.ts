import { Component, input } from '@angular/core'
import type { ChangeStackVm } from '../../providers/change-stack'
import { StackChangeComponent } from './stack-change.component'
import { StackLevelComponent } from './stack-level.component'

let nextId = 0

/**
 * The Stack section: where a change stands among its repository's open changes, from what the
 * stack stands on up to the changes that build on it, as the last collection found it. The
 * backend computes the stack (`ChangeStack`); a host's adapter lays it out (`changeStackVmOf`).
 */
@Component({
  selector: 'app-change-stack',
  imports: [StackChangeComponent, StackLevelComponent],
  templateUrl: './change-stack.component.html',
  styleUrl: './change-stack.component.scss',
})
export class ChangeStackComponent {
  readonly stack = input.required<ChangeStackVm>()
  protected readonly headingId = `change-stack-title-${nextId++}`
}
