import { Component, computed, input, ChangeDetectionStrategy } from '@angular/core'
import type { ProviderStatus } from '../../generated/activity'
import { providerStatusLabel } from './provider-status-label'
import { StatusDotComponent } from './status-dot.component'

@Component({
  selector: 'app-status-pill',
  imports: [StatusDotComponent],
  templateUrl: './status-pill.component.html',
  styleUrl: './status-pill.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
  host: {
    '[attr.data-status]': 'status()',
    '[class.size-sm]': 'isSmall()',
    '[class.size-md]': "size() === 'md' && !compact()",
    '[class.size-auto]': "size() === 'auto' && !compact()",
    '[class.no-dot]': '!showDot()',
  },
})
export class StatusPillComponent {
  readonly status = input.required<ProviderStatus>()
  readonly size = input<'auto' | 'sm' | 'md'>('auto')
  readonly label = input<string | null>(null)
  readonly compact = input(false)
  readonly showDot = input(true)

  protected readonly displayLabel = computed(
    () => this.label() ?? providerStatusLabel(this.status())
  )
  protected readonly isSmall = computed(() => this.compact() || this.size() === 'sm')
}
