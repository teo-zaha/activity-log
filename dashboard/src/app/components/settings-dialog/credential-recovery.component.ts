import { Component, input } from '@angular/core'
import type { WebFixAction } from '../../generated/activity'
import { ButtonComponent } from '../button/button.component'
import { ExternalLinkDirective } from '../external-link/external-link.directive'

@Component({
  selector: 'app-credential-recovery',
  imports: [ButtonComponent, ExternalLinkDirective],
  template: `
    @let setup = action();
    @if (setup.kind === 'createToken') {
      <div class="links">
        <a appButton size="sm" icon="open_in_new" [appExternalLink]="setup.url">{{
          setup.classicUrl ? 'Create fine-grained token' : 'Create token'
        }}</a>
        @if (setup.classicUrl; as classic) {
          <a appButton size="sm" variant="ghost" [appExternalLink]="classic">Classic token</a>
        }
      </div>
      <p>{{ setup.instructions }}</p>
    }
  `,
  styles: `
    :host {
      display: block;
    }
    .links {
      display: flex;
      flex-wrap: wrap;
      gap: 6px;
    }
    p {
      margin: 4px 0 0;
      font-size: 12px;
      color: var(--text-secondary);
    }
  `,
})
export class CredentialRecoveryComponent {
  readonly action = input.required<WebFixAction>()
}
