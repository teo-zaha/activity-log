import { A11yModule } from '@angular/cdk/a11y'
import { NgTemplateOutlet } from '@angular/common'
import {
  afterEveryRender,
  afterNextRender,
  Component,
  computed,
  effect,
  ElementRef,
  inject,
  Injector,
  input,
  linkedSignal,
} from '@angular/core'
import { mediaQuery } from '@signality/core/browser/media-query'
import { connectionKey } from '../../providers/connection'
import { getProviderAdapter } from '../../providers/provider-adapter'
import { ActivityService } from '../../services/activity.service'
import { downQuery } from '../../utils/breakpoints'
import { PageHeaderComponent } from '../page-header/page-header.component'
import { ButtonComponent } from '../button/button.component'
import { EmptyStateComponent } from '../empty-state/empty-state.component'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SettingsNavComponent } from './nav/settings-nav.component'
import { SettingsGeneralPaneComponent } from './general-pane/settings-general-pane.component'
import { SettingsLogsPaneComponent } from './logs-pane/settings-logs-pane.component'
import { SettingsAddSourcePaneComponent } from './add-source-pane/settings-add-source-pane.component'
import { SettingsSourcePaneComponent } from './source-pane/settings-source-pane.component'
import { SettingsPageActions } from './settings-page-actions'
import {
  connectionSelection,
  type SettingsProviderRow,
  type SettingsSelection,
} from './settings-dialog.types'

/** What each section says under its title; one short line at most. */
const DESCRIPTIONS: Partial<Record<SettingsSelection, string>> = {
  general: 'Applies at once and is kept for next time.',
  add: 'A source can be added more than once, such as two GitLab accounts.',
}

/**
 * The settings: the list of sections beside the open one, or on a phone the list as a page of
 * its own and each section as the next. The URL says which is open (`?settings=`). Shown in the
 * dialog on wide screens and in the page host on narrower ones.
 */
@Component({
  selector: 'app-settings-dialog',
  imports: [
    A11yModule,
    ButtonComponent,
    EmptyStateComponent,
    NgTemplateOutlet,
    PageHeaderComponent,
    ProviderIconComponent,
    SettingsNavComponent,
    SettingsGeneralPaneComponent,
    SettingsLogsPaneComponent,
    SettingsAddSourcePaneComponent,
    SettingsSourcePaneComponent,
  ],
  providers: [SettingsPageActions],
  templateUrl: './settings-dialog.component.html',
  styleUrl: './settings-dialog.component.scss',
  host: { '[class.is-page]': "chrome() === 'page'" },
})
export class SettingsDialogComponent {
  protected readonly activity = inject(ActivityService)
  protected readonly pageActions = inject(SettingsPageActions).template
  /** `page` fills the window it is in; `dialog` is a panel of its own. */
  readonly chrome = input<'dialog' | 'page'>('dialog')
  protected readonly isPhone = mediaQuery(downQuery('phone'))

  /** The URL's section; while the settings close, the URL has none and the last one stays. */
  private readonly selection = linkedSignal<SettingsSelection | null, SettingsSelection>({
    source: this.activity.settings,
    computation: (selection, previous) => selection ?? previous?.value ?? 'general',
  })
  /** Whether a phone shows the list of sections as the page. */
  protected readonly showsHome = computed(() => this.isPhone() && this.selection() === 'home')
  /** The open section; wider screens show General for the phone's list. */
  protected readonly section = computed<SettingsSelection>(() => {
    const selection = this.selection()
    return selection === 'home' ? 'general' : selection
  })
  protected readonly providerRows = computed<SettingsProviderRow[]>(() => {
    return this.activity.providerInfo().map(info => {
      const adapter = getProviderAdapter(info.connection.source)
      return {
        info,
        key: connectionKey(info.connection),
        label: info.label,
        kindLabel: adapter.label,
        tooltip: adapter.getStatusTooltip(info),
      }
    })
  })
  protected readonly selectedProvider = computed<SettingsProviderRow | null>(() => {
    const section = this.section()
    return this.providerRows().find(provider => section === `connection:${provider.key}`) ?? null
  })
  /**
   * A connection the URL names that is not among the sources (a stale link, a removed source):
   * `loading` until the sources are in.
   */
  protected readonly missingSource = computed<'loading' | 'missing' | null>(() => {
    if (!this.section().startsWith('connection:') || this.selectedProvider() !== null) return null
    const known = this.activity.providersLoaded() || this.activity.error() !== null
    return known ? 'missing' : 'loading'
  })
  protected readonly title = computed(() => {
    switch (this.section()) {
      case 'logs':
        return 'Logs'
      case 'add':
        return 'Add source'
      case 'general':
      case 'home':
        return 'Appearance'
      default:
        return (
          this.selectedProvider()?.label ??
          (this.missingSource() === 'loading' ? 'Source' : 'Source not found')
        )
    }
  })
  protected readonly description = computed(() => DESCRIPTIONS[this.section()] ?? null)

  protected readonly connectionSelection = connectionSelection

  constructor() {
    // On a phone the page that had focus is gone after moving between the list and a section:
    // the new page's title takes it, so it is read out.
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
    const injector = inject(Injector)
    let shown: SettingsSelection | null = null
    effect(() => {
      const page = this.isPhone() ? (this.showsHome() ? 'home' : this.section()) : null
      if (page !== null && shown !== null && page !== shown) {
        afterNextRender(
          () => host.querySelector<HTMLElement>('#settings-page-title, .home-title')?.focus(),
          { injector }
        )
      }
      shown = page
    })

    // A control that had focus can go away or be disabled (Clear with no logs left, Discard, a
    // removed list row): focus would fall to the page, out of the settings, where Tab leaves them
    // and Escape reaches no handler on a dialog. The section's title takes it instead.
    let hadFocus = false
    afterEveryRender(() => {
      const active = document.activeElement
      if (active !== null && host.contains(active) && !active.matches(':disabled')) {
        hadFocus = true
        return
      }
      const lost = active === null || active === document.body || host.contains(active)
      if (!lost || !hadFocus || this.activity.settings() === null) return
      host.querySelector<HTMLElement>('#settings-page-title, .home-title')?.focus()
    })
  }

  /** From the list: on a phone the section is a page of its own, Back returns to the list. */
  protected select(selection: SettingsSelection): void {
    void (this.isPhone()
      ? this.activity.enterSettings(selection)
      : this.activity.showSettings(selection))
  }

  protected back(): void {
    void this.activity.backToSettingsHome()
  }

  /** After a source is removed, and from a source that is not there: the list on a phone, else General. */
  protected afterRemove(): void {
    void (this.isPhone()
      ? this.activity.backToSettingsHome()
      : this.activity.showSettings('general'))
  }

  protected close(): void {
    void this.activity.closeSettings()
  }
}
