import { Component, inject } from '@angular/core'
import { EmptyStateComponent } from '../empty-state/empty-state.component'
import { ButtonComponent } from '../button/button.component'
import { SOURCES } from '../../generated/activity'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'

/** Shown instead of the views until a first source is added. */
@Component({
  selector: 'app-no-sources',
  imports: [EmptyStateComponent, ButtonComponent, ProviderIconComponent],
  templateUrl: './no-sources.component.html',
  styleUrl: './no-sources.component.scss',
})
export class NoSourcesComponent {
  private readonly settings = inject(SettingsDialogLauncher)

  protected readonly isDesktop = inject(RuntimeEnvironmentService).isTauri()
  protected readonly previews = SOURCES

  protected addSource(): void {
    this.settings.open('add')
  }
}
