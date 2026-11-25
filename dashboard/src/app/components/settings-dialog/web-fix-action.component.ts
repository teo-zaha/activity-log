import { Component, input } from '@angular/core'
import type { WebFixAction } from '../../generated/activity'
import { ButtonComponent } from '../button/button.component'
import { ExternalLinkDirective } from '../external-link/external-link.directive'

@Component({
  selector: 'app-web-fix-action',
  imports: [ButtonComponent, ExternalLinkDirective],
  template: `
    @let setup = action();
    @switch (setup.kind) {
      @case ('createToken') {
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
      @case ('feedHelp') {
        @if (setup.recovery; as recovery) {
          <p class="recovery">{{ recovery }}</p>
        }
        @if (setup.guides.length) {
          <details [open]="setup.recovery !== undefined">
            <summary>
              {{
                setup.recovery ? 'Calendar link instructions' : 'Where do I find my calendar link?'
              }}
            </summary>
            <ul>
              @for (guide of setup.guides; track guide.service) {
                <li>
                  @if (guide.url; as url) {
                    <a [appExternalLink]="url">{{ guide.label }}</a>
                  } @else {
                    <strong>{{ guide.label }}</strong>
                  }
                  <p>{{ guide.instructions }}</p>
                </li>
              }
            </ul>
          </details>
        }
      }
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
    .recovery {
      margin: 0 0 4px;
      color: var(--text-primary);
    }
    summary {
      cursor: pointer;
      font-size: 12px;
      font-weight: 600;
    }
    ul {
      margin: 6px 0 0;
      padding-inline-start: 20px;
    }
    li + li {
      margin-top: 8px;
    }
    a:not([appButton]) {
      color: var(--accent);
    }
  `,
})
export class WebFixActionComponent {
  readonly action = input.required<WebFixAction>()
}
