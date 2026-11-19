import { Directive, inject, input } from '@angular/core'
import { ActivityBackendClient } from '../../services/backend/backend-client'

/**
 * A link that opens outside the app: a new browser tab, or the default browser on desktop,
 * where the web view cannot open one itself. Only http(s) links are opened.
 */
@Directive({
  selector: 'a[appExternalLink]',
  host: {
    '[href]': 'appExternalLink()',
    target: '_blank',
    rel: 'noopener noreferrer',
    '(click)': 'open($event)',
    '(auxclick)': 'open($event)',
  },
})
export class ExternalLinkDirective {
  readonly appExternalLink = input.required<string>()
  private readonly backend = inject(ActivityBackendClient)

  protected open(event: MouseEvent): void {
    // A right click opens the context menu; the middle button opens like a click.
    if (event.type === 'auxclick' && event.button !== 1) return
    event.preventDefault()
    this.backend.openUrl(this.appExternalLink()).catch((error: unknown) => {
      console.error('Opening the link failed', error)
    })
  }
}
