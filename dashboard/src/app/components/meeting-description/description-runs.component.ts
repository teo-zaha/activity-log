import { Component, input } from '@angular/core'
import type { TextSegment } from '../../generated/activity'
import { ExternalLinkDirective } from '../external-link/external-link.directive'

/**
 * The runs of a description block: text, bold or italic, and links. Every run is an element:
 * white space between them would show under `pre-line`.
 */
@Component({
  selector: 'app-description-runs',
  imports: [ExternalLinkDirective],
  host: { style: 'display: contents' },
  template: `@for (segment of segments(); track $index) {
    @if (segment.kind === 'link') {
      <a class="description-link" [appExternalLink]="segment.href">{{ segment.text }}</a>
    } @else {
      <span [class.strong]="segment.strong" [class.emphasis]="segment.emphasis">{{
        segment.text
      }}</span>
    }
  }`,
  styleUrl: './description-runs.component.scss',
})
export class DescriptionRunsComponent {
  readonly segments = input.required<readonly TextSegment[]>()
}
