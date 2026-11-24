import { NgTemplateOutlet } from '@angular/common'
import { Component, input } from '@angular/core'
import type { StackChangeVm } from '../../providers/change-stack'
import { ExternalLinkDirective } from '../external-link/external-link.directive'

/**
 * One change of a stack: `!12 Title`, opening its page outside the app where the host gave one,
 * plain text otherwise. The stack labels the entry's own change as current and links to it
 * when its URL is available. The flex row drops the spaces between the parts, which
 * keep them apart for a screen reader.
 */
@Component({
  selector: 'app-stack-change',
  imports: [ExternalLinkDirective, NgTemplateOutlet],
  host: { style: 'display: contents' },
  template: `@let row = change();
    @let current = row.kind === 'current';
    @if (row.href; as href) {
      <a
        class="change is-link"
        [class.is-current]="current"
        [attr.aria-current]="current ? 'true' : null"
        [appExternalLink]="href"
        ><ng-container [ngTemplateOutlet]="parts"
      /></a>
    } @else {
      <span
        class="change"
        [class.is-current]="current"
        [attr.aria-current]="current ? 'true' : null"
        ><ng-container [ngTemplateOutlet]="parts"
      /></span>
    }
    <ng-template #parts>
      <span class="reference">{{ row.reference }}</span
      >&ngsp;
      <span class="title">{{ row.title }}</span>
      @if (current) {
        &ngsp;<span class="tag">This change</span>
      }
    </ng-template>`,
  styleUrl: './stack-change.component.scss',
})
export class StackChangeComponent {
  readonly change = input.required<StackChangeVm>()
}
