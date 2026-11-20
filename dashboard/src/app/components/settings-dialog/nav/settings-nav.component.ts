import { A11yModule } from '@angular/cdk/a11y'
import {
  Component,
  type ElementRef,
  computed,
  inject,
  input,
  output,
  viewChild,
} from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { THEMES, ThemeService } from '../../../services/theme.service'
import { AutoAnimateDirective } from '../../auto-animate/auto-animate.directive'
import { PageHeaderComponent } from '../../page-header/page-header.component'
import { ProviderIconComponent } from '../../provider-icon/provider-icon.component'
import { providerStatusLabel } from '../../ui-status/provider-status-label'
import { StatusDotComponent } from '../../ui-status/status-dot.component'
import {
  SETTINGS_SECTIONS,
  type SettingsProviderRow,
  type SettingsSelection,
} from '../settings-dialog.types'

/**
 * The settings sections and sources: a list beside the open section, or on a phone (`home`) the
 * settings' first page, with a close button and each row opening its section.
 */
@Component({
  selector: 'app-settings-nav',
  imports: [
    A11yModule,
    AutoAnimateDirective,
    MatIconModule,
    MatTooltipModule,
    PageHeaderComponent,
    ProviderIconComponent,
    StatusDotComponent,
  ],
  templateUrl: './settings-nav.component.html',
  styleUrl: './settings-nav.component.scss',
  host: { '[class.is-home]': 'home()' },
})
export class SettingsNavComponent {
  readonly selection = input.required<SettingsSelection>()
  readonly providers = input.required<readonly SettingsProviderRow[]>()
  /** The phone's list page instead of a column beside the section. */
  readonly home = input(false)
  readonly selected = output<SettingsSelection>()
  /** The home list's title, which takes focus when the list comes back on a phone. */
  readonly homeTitle = viewChild<ElementRef<HTMLElement>>('homeTitle')

  private readonly theme = inject(ThemeService).theme
  protected readonly themeLabel = computed(
    () => THEMES.find(option => option.id === this.theme())?.label ?? ''
  )
  protected readonly sections = SETTINGS_SECTIONS
  protected readonly providerStatusLabel = providerStatusLabel

  /** What a source's row says under its name: the reason it is not fine, else its status. */
  protected statusText(provider: SettingsProviderRow): string {
    const { status, reason } = provider.info
    return status !== 'available' && reason ? reason : providerStatusLabel(status)
  }
}
