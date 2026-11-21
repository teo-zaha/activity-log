import { A11yModule } from '@angular/cdk/a11y'
import {
  afterNextRender,
  Component,
  computed,
  effect,
  ElementRef,
  inject,
  Injector,
  viewChild,
  viewChildren,
} from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { Router } from '@angular/router'
import { AutoAnimateDirective } from '../auto-animate/auto-animate.directive'
import type { EntryUiAdapter } from '../../providers/types'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection } from '../../services/activity-selection.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import { ButtonComponent } from '../button/button.component'
import { EmptyStateComponent } from '../empty-state/empty-state.component'
import { IconButtonComponent } from '../button/icon-button.component'
import { ExternalLinkDirective } from '../external-link/external-link.directive'
import { MeetingDescriptionComponent } from '../meeting-description/meeting-description.component'
import { PageHeaderComponent } from '../page-header/page-header.component'
import { appLink, injectLinkBase, injectShareLink } from '../share/share-link'
import { SourceBadgeComponent } from '../source-badge/source-badge.component'
import { TopLayer } from '../../layouts/shell/top-layer.service'
import { NavigationMotionService } from '../../navigation-motion'
import { ShortcutService, ariaKeysOf } from '../../services/shortcut.service'

/**
 * The event drawer. A single event opens straight into its details; overlapping events open as a
 * list, and each row opens the same details with a way back and a stepper through the group. It
 * lies over whatever the shell shows, the phone's Settings tab included; on a phone it is a page
 * of its own, and the list and an event are pages in it.
 */
@Component({
  selector: 'app-event-detail',
  imports: [
    AutoAnimateDirective,
    A11yModule,
    ButtonComponent,
    EmptyStateComponent,
    ExternalLinkDirective,
    IconButtonComponent,
    MatIconModule,
    MeetingDescriptionComponent,
    PageHeaderComponent,
    SourceBadgeComponent,
  ],
  templateUrl: './event-detail.component.html',
  styleUrl: './event-detail.component.scss',
  host: {
    // Under the settings dialog the details are out of reach, as everything under them is.
    '[attr.inert]': "layers.top() === 'settingsDialog' ? '' : null",
  },
})
export class EventDetailComponent {
  private readonly data = inject(ActivityDataService)
  private readonly selection = inject(ActivitySelection)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly injector = inject(Injector)
  private readonly router = inject(Router)
  private readonly shareLink = injectShareLink()
  private readonly linkBase = injectLinkBase()
  /** On a phone the details are a page of their own, left with a back arrow. */
  protected readonly isPhone = inject(BreakpointState).phone
  protected readonly motion = inject(NavigationMotionService)
  protected readonly layers = inject(TopLayer)
  private readonly shortcuts = inject(ShortcutService)
  /**
   * The keys of the controls that do what a shortcut does: the group's back button goes Back;
   * the close button and the phone's back arrow, which closes too, answer Escape and Back.
   */
  protected readonly keys = {
    back: this.shortcuts.ariaKeys('app.back'),
    close: this.shortcuts.ariaKeys('layer.close'),
    closeOrBack: ariaKeysOf([
      this.shortcuts.ariaKeys('layer.close'),
      this.shortcuts.ariaKeys('app.back'),
    ]),
  }
  private readonly title = viewChild<ElementRef<HTMLElement>>('detailTitle')
  private readonly listTitle = viewChild<ElementRef<HTMLElement>>('listTitle')
  private readonly body = viewChild<ElementRef<HTMLElement>>('body')
  private readonly rows = viewChildren<ElementRef<HTMLButtonElement>>('row')
  private readonly flyout = viewChild<ElementRef<HTMLElement>>('flyout')
  private previouslyFocusedElement: HTMLElement | null = null
  private wasOpen = false

  /** The listed group's loaded events; null without a group. */
  protected readonly group = this.selection.selectedEntries
  /** The open event, once its entry is loaded. */
  protected readonly detail = this.selection.selectedEntry
  /** Where the open event sits in the group; null while the list shows, and without a group. */
  protected readonly openIndex = computed(() => {
    const group = this.group()
    const entry = this.detail()
    const index = group && entry ? group.indexOf(entry) : -1
    return index < 0 ? null : index
  })
  protected readonly isOpen = this.viewState.detailsOpen
  /**
   * What shows for details the URL names but the entries do not hold: `loading` until the log is
   * in, then `missing` (another range, or the entry is gone). Null when there is something to show.
   */
  protected readonly pending = computed<'loading' | 'missing' | null>(() => {
    const eventMissing = this.viewState.openEventKey() !== null && this.detail() === null
    const groupMissing = this.group()?.length === 0
    if (!eventMissing && !groupMissing) return null
    const status = this.data.collectionStatus().kind
    const notYet =
      this.data.isLoading() ||
      (status === 'idle' && this.data.meta() === null && !this.data.hasNoSources())
    return notYet ? 'loading' : 'missing'
  })
  /** The group position last shown, to move focus when the list and an event swap. */
  private lastIndex: number | null = null

  constructor() {
    effect(() => {
      const isOpen = this.isOpen()
      const index = this.openIndex()

      // The details take focus as they open, on their title, so it is read out first.
      if (isOpen && !this.wasOpen) {
        const activeElement = document.activeElement
        this.previouslyFocusedElement = activeElement instanceof HTMLElement ? activeElement : null
        this.moveFocus(() => this.titleOnScreen())
      }

      if (!isOpen && this.wasOpen) {
        this.restoreFocus()
      }

      // In a group, the row that was used is gone once its event opens, and the event's controls
      // are gone once the list is back: focus moves to the new heading or the row it came from.
      if (isOpen && this.wasOpen && this.group() !== null) {
        const from = this.lastIndex
        if (from === null && index !== null) {
          this.moveFocus(() => this.title()?.nativeElement)
        } else if (from !== null && index === null) {
          // The row, unless the event left the group: then the page that says so.
          this.moveFocus(() => (this.rows()[from] ?? this.title() ?? this.flyout())?.nativeElement)
        }
      }

      this.wasOpen = isOpen
      this.lastIndex = index
    })

    // The settings dialog closed over the details: they take the focus it had.
    this.layers.whenExposed('details', () => this.focusIfOutside())

    // While open, the details are a layer: Escape and Back are theirs while they are on top (the
    // settings dialog opens over them, the phone's Settings tab lies under them). A field or
    // dialog that handles Escape first keeps it. On a phone they hide the view under them.
    effect(onCleanup => {
      if (!this.isOpen()) return
      const onTop = (): boolean => this.layers.top() === 'details'
      const off = [
        this.shortcuts.openLayer({ kind: 'page', hidesRoute: this.isPhone() }),
        this.shortcuts.bind('layer.close', () => this.close(), onTop),
        this.shortcuts.bind('app.back', () => this.back(), onTop),
      ]
      onCleanup(() => off.forEach(close => close()))
    })
  }

  /**
   * Moves focus after the next render, only while the details are on top: under the settings
   * dialog they are `inert`, and a change in them (an update that removes or brings back the open
   * event) must not pull focus out of the dialog. The dialog's close hands it over
   * (`focusIfOutside`).
   */
  private moveFocus(to: () => HTMLElement | undefined): void {
    afterNextRender(
      () => {
        if (this.layers.top() === 'details') to()?.focus()
      },
      { injector: this.injector }
    )
  }

  /** The page's title, or the panel when the page has none (loading, not found). */
  private titleOnScreen(): HTMLElement | undefined {
    return (this.title() ?? this.listTitle() ?? this.flyout())?.nativeElement
  }

  private focusIfOutside(): void {
    const flyout = this.flyout()?.nativeElement
    if (flyout !== undefined && !flyout.contains(document.activeElement)) {
      this.titleOnScreen()?.focus()
    }
  }

  /** Back from an event of a group is its list; from anything else it closes the details. */
  private back(): void {
    if (this.openIndex() !== null) this.backToList()
    else this.close()
  }

  protected close(): void {
    void this.viewState.closeDetail()
  }

  protected open(index: number): void {
    const entry = this.group()?.[index]
    if (entry) void this.viewState.showInGroup(entry.trackKey)
  }

  protected backToList(): void {
    void this.viewState.backToGroupList()
  }

  /** Moves to the previous or next event of the group; false at either end. */
  protected step(delta: -1 | 1): boolean {
    const group = this.group()
    const index = this.openIndex()
    const next = group && index !== null ? group[index + delta] : undefined
    if (!next) return false
    void this.viewState.stepInGroup(next.trackKey)
    return true
  }

  /**
   * In the details, ArrowUp/ArrowDown scroll first and step to the previous or next event once
   * the details are scrolled to that end. In the list, they move between rows.
   */
  protected onArrow(event: Event, delta: -1 | 1): void {
    if (!this.group()) {
      return
    }

    if (this.openIndex() === null) {
      const rows = this.rows().map(row => row.nativeElement)
      const current = rows.findIndex(row => row === document.activeElement)
      const next = current === -1 ? 0 : current + delta
      if (rows[next]) {
        event.preventDefault()
        rows[next].focus()
      }
      return
    }

    const body = this.body()?.nativeElement
    const canScroll =
      body !== undefined &&
      (delta < 0 ? body.scrollTop > 0 : body.scrollTop + body.clientHeight < body.scrollHeight - 1)
    if (canScroll) {
      // Focus inside the details scrolls them natively; from the header, scroll them here.
      if (!body.contains(document.activeElement)) {
        event.preventDefault()
        body.scrollBy({ top: delta * 40 })
      }
      return
    }

    if (this.step(delta)) {
      event.preventDefault()
    }
  }

  /** Shares the link to this one event in the range on screen, without its group or filter. */
  /** The link names the range shown, the URL's or the collected one, so it opens with the event in it. */
  protected shareEntry(entry: EntryUiAdapter): Promise<void> {
    const [from, to] = [this.viewState.dateFrom(), this.viewState.dateTo()]
    const range = from && to ? { from, to } : {}
    return this.shareLink(
      appLink(this.router, [], { ...range, event: entry.trackKey }, this.linkBase)
    )
  }

  protected formatDateTime(ts: string): string {
    return new Date(ts).toLocaleString('en-GB', {
      weekday: 'long',
      year: 'numeric',
      month: 'long',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    })
  }

  private restoreFocus(): void {
    const element = this.previouslyFocusedElement
    this.previouslyFocusedElement = null

    if (!element?.isConnected) {
      return
    }

    queueMicrotask(() => {
      element.focus()
    })
  }
}
