import { Component, computed, inject, input, signal } from '@angular/core'
import type {
  ConnectionId,
  LocalApp,
  LocalFixTarget,
  ProviderFixAction,
  SystemSettingsPane,
} from '../../generated/activity'
import { type ConnectionKey, connectionKey } from '../../providers/connection'
import { ActivityBackendClient } from '../../services/backend/backend-client'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'
import { failureWhile } from '../../services/failure'
import { NoticeService } from '../../services/notice.service'
import { ButtonComponent } from '../button/button.component'
import { ExternalLinkDirective } from '../external-link/external-link.directive'

const PANE_LABELS: Readonly<Record<SystemSettingsPane, string>> = {
  automation: 'Automation settings',
  filesAndFolders: 'Files & Folders settings',
}
const APP_LABELS: Readonly<Record<LocalApp, string>> = { outlook: 'Outlook' }

@Component({
  selector: 'app-provider-fix-action',
  imports: [ButtonComponent, ExternalLinkDirective],
  template: `
    @let fix = action();
    @if (fix.kind === 'local') {
      @if (isDesktop) {
        <button
          appButton
          size="sm"
          icon="open_in_new"
          [busy]="opening()"
          [disabled]="opening()"
          (click)="openLocal(fix.target)"
        >
          Open {{ targetLabel(fix.target) }}
        </button>
      }
    } @else {
      @let setup = fix.action;
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
                  setup.recovery
                    ? 'Calendar link instructions'
                    : 'Where do I find my calendar link?'
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
export class ProviderFixActionComponent {
  private readonly backend = inject(ActivityBackendClient)
  private readonly notices = inject(NoticeService)
  protected readonly isDesktop = inject(RuntimeEnvironmentService).isTauri()
  private readonly openingConnections = signal<ReadonlySet<ConnectionKey>>(new Set())

  readonly connection = input.required<ConnectionId>()
  readonly action = input.required<ProviderFixAction>()
  protected readonly opening = computed(() =>
    this.openingConnections().has(connectionKey(this.connection()))
  )

  protected targetLabel(target: LocalFixTarget): string {
    switch (target.kind) {
      case 'systemSettings':
        return PANE_LABELS[target.pane]
      case 'openApp':
        return APP_LABELS[target.app]
    }
  }

  protected async openLocal(target: LocalFixTarget): Promise<void> {
    if (!this.isDesktop || this.opening()) return
    const connection = this.connection()
    const key = connectionKey(connection)
    this.openingConnections.update(keys => new Set(keys).add(key))
    this.notices.clear('localFix')
    const label = this.targetLabel(target)
    try {
      await this.backend.openLocalFix({ connection, target })
    } catch (error) {
      this.notices.show({ origin: 'localFix', failure: failureWhile(`Opening ${label}`, error) })
    } finally {
      this.openingConnections.update(keys => {
        const remaining = new Set(keys)
        remaining.delete(key)
        return remaining
      })
    }
  }
}
