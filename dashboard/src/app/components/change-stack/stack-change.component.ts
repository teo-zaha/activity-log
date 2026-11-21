import { Component, input } from '@angular/core'
import type { StackChangeVm } from '../../providers/change-stack'
import { ExternalLinkDirective } from '../external-link/external-link.directive'

/**
 * One change of a stack: `!12 Title`. Another change opens its page outside the app; the entry's
 * own is marked as the one on screen and is no link, and a change without a page is plain text.
 * The flex row drops the spaces between the parts, which keep them apart for a screen reader.
 */
@Component({
  selector: 'app-stack-change',
  imports: [ExternalLinkDirective],
  host: { style: 'display: contents' },
  template: `@let row = change();
    @switch (row.kind) {
      @case ('current') {
        <span class="change is-current">
          <span class="reference">{{ row.reference }}</span
          >&ngsp; <span class="title">{{ row.title }}</span
          >&ngsp;
          <span class="tag">This change</span>
        </span>
      }
      @case ('member') {
        @if (row.href; as href) {
          <a class="change is-link" [appExternalLink]="href">
            <span class="reference">{{ row.reference }}</span
            >&ngsp;
            <span class="title">{{ row.title }}</span>
          </a>
        } @else {
          <span class="change">
            <span class="reference">{{ row.reference }}</span
            >&ngsp;
            <span class="title">{{ row.title }}</span>
          </span>
        }
      }
    }`,
  styleUrl: './stack-change.component.scss',
})
export class StackChangeComponent {
  readonly change = input.required<StackChangeVm>()
}
