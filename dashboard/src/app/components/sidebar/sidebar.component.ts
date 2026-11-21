import { NgTemplateOutlet } from '@angular/common'
import { Component, type TemplateRef, inject, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { RouterLink, RouterLinkActive } from '@angular/router'
import { NAV_GROUPS } from '../../models/nav'
import { VIEW_SHORTCUTS } from '../../models/shortcuts'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import { ShortcutService } from '../../services/shortcut.service'
import type { ViewMode } from '../../utils/app-url'
import { viewCommands } from '../../utils/app-url'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'

/**
 * The navigation, drawn from `NAV_GROUPS`, and Settings: a column on wide screens, a rail of icons
 * on narrower ones, and on a phone the bottom bar, with Settings as its last tab.
 */
@Component({
  selector: 'app-sidebar',
  imports: [MatIconModule, MatTooltipModule, NgTemplateOutlet, RouterLink, RouterLinkActive],
  templateUrl: './sidebar.component.html',
  styleUrl: './sidebar.component.scss',
})
export class SidebarComponent {
  /** The layout's part of the sidebar, such as the views' source chips; none on other pages. */
  readonly sources = input<TemplateRef<unknown> | null>(null)

  private readonly viewState = inject(ActivityViewStateService)
  private readonly settings = inject(SettingsDialogLauncher)
  private readonly breakpoints = inject(BreakpointState)

  protected readonly groups = NAV_GROUPS
  /** The views' and Settings' keys, for `aria-keyshortcuts`: the shell's fallbacks run them. */
  protected readonly shortcuts = inject(ShortcutService)
  protected readonly viewShortcuts = VIEW_SHORTCUTS
  protected readonly isPhone = this.breakpoints.phone
  protected readonly isRail = this.breakpoints.rail
  /** On a phone the settings are the bottom bar's last tab: its views are not current then. */
  protected readonly settingsTab = this.settings.pageOpen

  /** A calendar view opens on the day shown last. */
  protected viewLink(view: ViewMode): string[] {
    return viewCommands(view, this.viewState.selectedDay())
  }

  /** Tapping the open tab again keeps the page it is on. */
  protected showSettingsTab(): void {
    if (!this.settingsTab()) this.openSettings()
  }

  protected openSettings(): void {
    this.settings.open()
  }
}
