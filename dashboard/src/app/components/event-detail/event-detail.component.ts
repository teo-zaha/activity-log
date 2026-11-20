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
import { mediaQuery } from '@signality/core/browser/media-query'
import { AutoAnimateDirective } from '../auto-animate/auto-animate.directive'
import type { EntryUiAdapter } from '../../providers/types'
import { ActivityService } from '../../services/activity.service'
import { ButtonComponent } from '../button/button.component'
import { EmptyStateComponent } from '../empty-state/empty-state.component'
import { IconButtonComponent } from '../button/icon-button.component'
import { ExternalLinkDirective } from '../external-link/external-link.directive'
import { MeetingDescriptionComponent } from '../meeting-description/meeting-description.component'
import { PageHeaderComponent } from '../page-header/page-header.component'
import { appLink, injectLinkBase, injectShareLink } from '../share/share-link'
import { SourceBadgeComponent } from '../source-badge/source-badge.component'
import { downQuery } from '../../utils/breakpoints'

/**
 * The event drawer. A single event opens straight into its details; overlapping events open as a
 * list, and each row opens the same details with a way back and a stepper through the group.
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
    // On the document, so Escape closes even when a click left focus on nothing.
    '(document:keydown.escape)': 'onEscape($event)',
  },
})
export class EventDetailComponent {
  protected readonly activity = inject(ActivityService)
  private readonly injector = inject(Injector)
  private readonly router = inject(Router)
  private readonly shareLink = injectShareLink()
  private readonly linkBase = injectLinkBase()
  /** On a phone the details are a page of their own, left with a back arrow. */
  protected readonly isPhone = mediaQuery(downQuery('phone'))
  private readonly title = viewChild<ElementRef<HTMLElement>>('detailTitle')
  private readonly body = viewChild<ElementRef<HTMLElement>>('body')
  private readonly rows = viewChildren<ElementRef<HTMLButtonElement>>('row')
  private readonly flyout = viewChild<ElementRef<HTMLElement>>('flyout')
  private previouslyFocusedElement: HTMLElement | null = null
  private wasOpen = false

  /** The listed group's loaded events; null without a group. */
  protected readonly group = this.activity.selectedEntries
  /** The open event, once its entry is loaded. */
  protected readonly detail = this.activity.selectedEntry
  /** Where the open event sits in the group; null while the list shows, and without a group. */
  protected readonly openIndex = computed(() => {
    const group = this.group()
    const entry = this.detail()
    const index = group && entry ? group.indexOf(entry) : -1
    return index < 0 ? null : index
  })
  protected readonly isOpen = computed(
    () => this.activity.openEventKey() !== null || this.activity.openGroupKeys().length > 0
  )
  /**
   * What shows for details the URL names but the entries do not hold: `loading` until the log is
   * in, then `missing` (another range, or the entry is gone). Null when there is something to show.
   */
  protected readonly pending = computed<'loading' | 'missing' | null>(() => {
    const eventMissing = this.activity.openEventKey() !== null && this.detail() === null
    const groupMissing = this.group()?.length === 0
    if (!eventMissing && !groupMissing) return null
    const status = this.activity.collectionStatus().kind
    const notYet =
      this.activity.isLoading() ||
      (status === 'idle' && this.activity.meta() === null && !this.activity.hasNoSources())
    return notYet ? 'loading' : 'missing'
  })
  /** The group position last shown, to move focus when the list and an event swap. */
  private lastIndex: number | null = null

  constructor() {
    effect(() => {
      const isOpen = this.isOpen()
      const index = this.openIndex()

      if (isOpen && !this.wasOpen) {
        const activeElement = document.activeElement
        this.previouslyFocusedElement = activeElement instanceof HTMLElement ? activeElement : null
      }

      if (!isOpen && this.wasOpen) {
        this.restoreFocus()
      }

      // In a group, the row that was used is gone once its event opens, and the event's controls
      // are gone once the list is back: focus moves to the new heading or the row it came from.
      if (isOpen && this.wasOpen && this.group() !== null) {
        const from = this.lastIndex
        if (from === null && index !== null) {
          afterNextRender(() => this.title()?.nativeElement.focus(), { injector: this.injector })
        } else if (from !== null && index === null) {
          // The row, unless the event left the group: then the page that says so.
          afterNextRender(
            () => (this.rows()[from] ?? this.title() ?? this.flyout())?.nativeElement.focus(),
            { injector: this.injector }
          )
        }
      }

      this.wasOpen = isOpen
      this.lastIndex = index
    })
  }

  /**
   * Only the layer on top closes: the settings open over the details, and a dialog over either
   * handles Escape first (and marks it handled).
   */
  protected onEscape(event: Event): void {
    if (!this.isOpen() || event.defaultPrevented || this.activity.settings() !== null) return
    event.preventDefault()
    this.close()
  }

  protected close(): void {
    void this.activity.closeDetail()
  }

  protected open(index: number): void {
    const entry = this.group()?.[index]
    if (entry) void this.activity.showInGroup(entry.trackKey)
  }

  protected backToList(): void {
    void this.activity.backToGroupList()
  }

  /** Moves to the previous or next event of the group; false at either end. */
  protected step(delta: -1 | 1): boolean {
    const group = this.group()
    const index = this.openIndex()
    const next = group && index !== null ? group[index + delta] : undefined
    if (!next) return false
    void this.activity.stepInGroup(next.trackKey)
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
    const [from, to] = [this.activity.dateFrom(), this.activity.dateTo()]
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
