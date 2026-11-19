import { booleanAttribute, Component, computed, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'

export type FeedbackTone = 'info' | 'ok' | 'warn' | 'error'

const ICONS: Readonly<Record<FeedbackTone, string>> = {
  info: 'info',
  ok: 'check_circle',
  warn: 'warning_amber',
  error: 'error_outline',
}

/**
 * A status message in the colours of its tone: `<app-feedback tone="error">Could not load</app-feedback>`.
 * Errors are announced at once (`role="alert"`), the rest politely. An element marked
 * `feedbackAction` goes at the end, such as a Retry button.
 */
@Component({
  selector: 'app-feedback',
  imports: [MatIconModule],
  template: `
    <mat-icon>{{ icon() }}</mat-icon>
    <span class="text"><ng-content /></span>
    <ng-content select="[feedbackAction]" />
  `,
  styleUrl: './feedback.component.scss',
  host: {
    '[attr.data-tone]': 'tone()',
    '[attr.role]': "tone() === 'error' ? 'alert' : 'status'",
    '[class.is-block]': 'block()',
  },
})
export class FeedbackComponent {
  readonly tone = input<FeedbackTone>('info')
  /** A full-width row, with room for an action at the end. */
  readonly block = input(false, { transform: booleanAttribute })

  protected readonly icon = computed(() => ICONS[this.tone()])
}
