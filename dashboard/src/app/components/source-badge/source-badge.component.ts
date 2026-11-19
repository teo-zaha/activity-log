import { booleanAttribute, Component, input } from '@angular/core'
import type { Source } from '../../generated/activity'
import type { EntryBadgeSplit } from '../../providers/types'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'

/** A source's icon and name in its colours, such as `GIT` on an entry. */
@Component({
  selector: 'app-source-badge',
  imports: [ProviderIconComponent],
  template: `<app-provider-icon class="icon" [source]="source()" [fallbackIcon]="icon() ?? null" />
    <span class="label">{{ label() }}</span>
    @if (split(); as split) {
      <span class="split" [attr.data-src]="split.source">{{ split.label }}</span>
    }`,
  styleUrl: './source-badge.component.scss',
  host: {
    '[attr.data-src]': 'source()',
    '[attr.data-size]': 'size()',
    '[class.icon-only-on-phone]': 'iconOnlyOnPhone()',
  },
})
export class SourceBadgeComponent {
  readonly source = input.required<Source>()
  readonly label = input.required<string>()
  /** A second source sharing the badge, in its own colours after a slanted split. */
  readonly split = input<EntryBadgeSplit>()
  /** The glyph shown when the source has no logo. */
  readonly icon = input<string>()
  /** `sm` in list rows, `md` in the event details. */
  readonly size = input<'sm' | 'md'>('md')
  /** On a phone only the icon shows; the name stays for screen readers. */
  readonly iconOnlyOnPhone = input(false, { transform: booleanAttribute })
}
