import { Component, inject, ChangeDetectionStrategy } from '@angular/core'
import { MatDialog, MatDialogModule } from '@angular/material/dialog'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import type { Source } from '../../generated/activity'
import { ActivityService } from '../../services/activity.service'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SettingsDialogComponent } from '../settings-dialog/settings-dialog.component'

@Component({
  selector: 'app-sidebar',
  imports: [MatDialogModule, MatIconModule, MatTooltipModule, ProviderIconComponent],
  templateUrl: './sidebar.component.html',
  styleUrl: './sidebar.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SidebarComponent {
  protected readonly activity = inject(ActivityService)
  private readonly dialog = inject(MatDialog)

  protected isActive(id: Source): boolean {
    return this.activity.activeSources().has(id)
  }

  protected openSettings(): void {
    this.dialog.open(SettingsDialogComponent, {
      panelClass: 'settings-dialog-panel',
      width: 'min(1040px, calc(100vw - 32px))',
      maxWidth: 'calc(100vw - 32px)',
      maxHeight: 'calc(100vh - 32px)',
      autoFocus: false,
      restoreFocus: false,
    })
  }
}
