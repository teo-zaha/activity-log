import { Component, input } from '@angular/core'
import type { ProviderStatus } from '../../generated/activity'

@Component({
  selector: 'app-status-dot',
  template: '',
  styleUrl: './status-dot.component.scss',
  host: {
    'aria-hidden': 'true',
    '[attr.data-status]': 'status()',
    '[class.size-sm]': "size() === 'sm'",
    '[class.no-glow]': '!glow()',
  },
})
export class StatusDotComponent {
  readonly status = input.required<ProviderStatus>()
  readonly size = input<'sm' | 'md'>('md')
  readonly glow = input(true)
}
