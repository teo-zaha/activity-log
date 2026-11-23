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
  viewChild,
} from '@angular/core'
import { connectionKey, isSource } from '../../providers/connection'
import { getProviderAdapter } from '../../providers/provider-adapter'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import { ShortcutService } from '../../services/shortcut.service'
import { NavigationMotionService } from '../../navigation-motion'
import { PageHeaderComponent } from '../page-header/page-header.component'
import { ButtonComponent } from '../button/button.component'
import { EmptyStateComponent } from '../empty-state/empty-state.component'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SettingsNavComponent } from './nav/settings-nav.component'
import { SettingsGeneralPaneComponent } from './general-pane/settings-general-pane.component'
import { SettingsLogsPaneComponent } from './logs-pane/settings-logs-pane.component'
import { SettingsAddSourcePaneComponent } from './add-source-pane/settings-add-source-pane.component'
import { SettingsSourcePaneComponent } from './source-pane/settings-source-pane.component'
import { RemovedSourceDraftComponent } from './removed-source-draft/removed-source-draft.component'
import { SourceDrafts } from './source-pane/source-drafts.service'
import { SettingsPageActions } from './settings-page-actions'
import {
  isSettingsPage,
  type SettingsProviderRow,
  type SettingsPage,
  type SettingsSelection,
} from './settings-dialog.types'

/** What each section says under its title; one short line at most. */
const DESCRIPTIONS: Partial<Record<SettingsPage, string>> = {
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
    RemovedSourceDraftComponent,
  ],
  providers: [SettingsPageActions],
  templateUrl: './settings-dialog.component.html',
  styleUrl: './settings-dialog.component.scss',
  host: { '[class.is-page]': "chrome() === 'page'" },
})
export class SettingsDialogComponent {
  private readonly data = inject(ActivityDataService)
  private readonly viewState = inject(ActivityViewStateService)
  protected readonly pageActions = inject(SettingsPageActions).template
  /** `page` fills the window it is in; `dialog` is a panel of its own. */
  readonly chrome = input<'dialog' | 'page'>('dialog')
  protected readonly isPhone = inject(BreakpointState).phone
  protected readonly motion = inject(NavigationMotionService)
  private readonly shortcuts = inject(ShortcutService)
  /**
   * The keys of the controls that do what a shortcut does: the page's back arrow goes Back
   * (`SettingsPageHostComponent`), the dialog's close button answers Escape (the dialog's own).
   */
  protected readonly keys = {
    back: this.shortcuts.ariaKeys('app.back'),
  }
  private readonly nav = viewChild(SettingsNavComponent)
  private readonly pageTitle = viewChild<ElementRef<HTMLElement>>('pageTitle')

  /** The URL's section; while the settings close, the URL has none and the last one stays. */
  private readonly selection = linkedSignal<SettingsSelection | null, SettingsSelection>({
    source: this.viewState.settings,
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
    return this.data.providerInfo().map(info => {
      const adapter = getProviderAdapter(info.connection.source)
      return {
        info,
        key: connectionKey(info.connection),
        kindLabel: adapter.label,
        tooltip: adapter.getStatusTooltip(info),
      }
    })
  })
  protected readonly selectedProvider = computed<SettingsProviderRow | null>(() => {
    const section = this.section()
    return this.providerRows().find(provider => section === provider.key) ?? null
  })
  /**
   * A connection the URL names that is not among the sources (a stale link, a removed source):
   * `loading` until the sources are in.
   */
  protected readonly missingSource = computed<'loading' | 'missing' | null>(() => {
    if (isSettingsPage(this.section()) || this.selectedProvider() !== null) return null
    const known = this.data.providersLoaded() || this.data.connection().status.kind === 'failed'
    return known ? 'missing' : 'loading'
  })
  private readonly drafts = inject(SourceDrafts)

  /**
   * A source the sources no longer list that still has unsaved changes (another program removed
   * it while they were made): they stay reachable, to discard or to save into the source added
   * again.
   */
  protected readonly removedDraft = computed(() => {
    const section = this.section()
    if (this.missingSource() !== 'missing' || isSettingsPage(section)) return null
    const draft = this.drafts.get(section)
    const slash = section.indexOf('/')
    const source = section.slice(0, slash)
    if (draft === undefined || !isSource(source)) return null
    const connection = { source, name: section.slice(slash + 1) }
    return { connection, label: draft.label ?? getProviderAdapter(source).label }
  })

  protected readonly title = computed(() => {
    switch (this.section()) {
      case 'logs':
        return 'Logs'
      case 'add':
        return 'Add source'
      case 'general':
      case 'home':
        return 'General'
      default:
        return (
          this.selectedProvider()?.info.label ??
          (this.missingSource() === 'loading' ? 'Source' : 'Source not found')
        )
    }
  })
  protected readonly description = computed(() => {
    const section = this.section()
    return isSettingsPage(section) ? (DESCRIPTIONS[section] ?? null) : null
  })

  constructor() {
    // On a phone the page that had focus is gone after moving between the list and a section:
    // the new page's title takes it, so it is read out. The page that leaves is still in the DOM
    // while it slides out; the queries no longer hold it.
    const host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
    const injector = inject(Injector)
    let shown: SettingsSelection | null = null
    effect(() => {
      const page = this.isPhone() ? (this.showsHome() ? 'home' : this.section()) : null
      if (page !== null && shown !== null && page !== shown) {
        afterNextRender(() => this.titleOnScreen()?.focus(), { injector })
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
      if (!lost || !hadFocus || this.viewState.settings() === null) return
      this.titleOnScreen()?.focus()
    })
  }

  /** Focus to the open section's title, or to the phone's list title. */
  focusTitle(): void {
    this.titleOnScreen()?.focus()
  }

  /** The open section's title, or the phone's list title. */
  private titleOnScreen(): HTMLElement | undefined {
    return (this.pageTitle() ?? this.nav()?.homeTitle())?.nativeElement
  }

  /** From the list: on a phone the section is a page of its own, Back returns to the list. */
  protected select(selection: SettingsSelection): void {
    void (this.isPhone()
      ? this.viewState.enterSettings(selection)
      : this.viewState.showSettings(selection))
  }

  protected back(): void {
    void this.viewState.backToSettingsHome()
  }

  /** Back on the phone's page (`app.back`): from a section to the list, from the list out. */
  pageBack(): void {
    if (this.showsHome()) this.close()
    else this.back()
  }

  /** After a source is removed, and from a source that is not there: the list on a phone, else General. */
  protected afterRemove(): void {
    void (this.isPhone()
      ? this.viewState.backToSettingsHome()
      : this.viewState.showSettings('general'))
  }

  protected close(): void {
    void this.viewState.closeSettings()
  }
}
