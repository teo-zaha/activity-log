import {
  Component,
  type Signal,
  type TemplateRef,
  afterNextRender,
  computed,
  inject,
} from '@angular/core'
import { RouterOutlet } from '@angular/router'
import { EventDetailComponent } from '../../components/event-detail/event-detail.component'
import { HeaderComponent } from '../../components/header/header.component'
import { SettingsDialogLauncher } from '../../components/settings-dialog/settings-dialog-launcher.service'
import { SettingsPageHostComponent } from '../../components/settings-dialog/settings-page-host.component'
import { SidebarComponent } from '../../components/sidebar/sidebar.component'
import { ActivityService } from '../../services/activity.service'
import { dismissSplash } from '../../utils/splash'
import { type ShellSlotName, ShellSlots } from './shell-slots'

/**
 * The frame every page is in: the header (brand, Export, Share), the navigation, the main content
 * with the routed layout in it, the phone's Settings tab and the event details over all of it.
 * The layout on screen fills the header row and the sidebar through `ShellSlots`.
 */
@Component({
  selector: 'app-shell-layout',
  imports: [
    EventDetailComponent,
    HeaderComponent,
    RouterOutlet,
    SettingsPageHostComponent,
    SidebarComponent,
  ],
  providers: [ShellSlots],
  templateUrl: './shell-layout.component.html',
  styleUrl: './shell-layout.component.scss',
})
export class ShellLayoutComponent {
  protected readonly activity = inject(ActivityService)
  private readonly slots = inject(ShellSlots)
  /**
   * On a phone the settings are a tab: they take the view's place in the main content, and the
   * slots stay empty, since what a layout puts there (the range, the chips) is for its views.
   */
  protected readonly settingsTab = inject(SettingsDialogLauncher).pageOpen
  /** The event details are modal at every size: what lies under them is `inert`. */
  protected readonly underDetails = this.activity.detailsOpen
  protected readonly headerSlot = this.slot('header')
  protected readonly sidebarSlot = this.slot('sidebar')

  constructor() {
    // The header is in the DOM now, the splash's brand flies into it; the data streams in later.
    afterNextRender(() => dismissSplash())
  }

  private slot(name: ShellSlotName): Signal<TemplateRef<unknown> | null> {
    const template = this.slots.template(name)
    return computed(() => (this.settingsTab() ? null : template()))
  }
}
