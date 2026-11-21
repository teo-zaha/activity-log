import { DOCUMENT } from '@angular/common'
import {
  Component,
  DestroyRef,
  ElementRef,
  afterRenderEffect,
  computed,
  inject,
  signal,
  viewChild,
} from '@angular/core'
import { windowSize } from '@signality/core/browser/window-size'
import { elementSize } from '@signality/core/elements/element-size'
import { MatIconModule } from '@angular/material/icon'
import { interval } from '@signality/core/scheduling/interval'
import { AutoAnimateDirective } from '../auto-animate/auto-animate.directive'
import { ButtonComponent } from '../button/button.component'
import { IconButtonComponent } from '../button/icon-button.component'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection } from '../../services/activity-selection.service'
import { retryable } from '../../services/collection-failure'
import { dayRangeLabel } from '../../utils/local-day'

type BannerTone = 'info' | 'warn' | 'error'

/** How long a reconnect keeps the state on screen before it counts as out of date. */
const STALE_AFTER_MS = 5000

/**
 * How far up from the bottom of the window the pill or the card on screen reaches, which the
 * notices keep clear of (`styles.scss`), so neither covers the other.
 */
export const STATUS_CLEARANCE = '--collection-status-clearance'

interface CollectionBannerVm {
  readonly tone: BannerTone
  readonly icon: string
  readonly title: string
  readonly message: string
  readonly busy: boolean
  /** Asking again could end otherwise (`retryable`). */
  readonly retry: boolean
}

@Component({
  selector: 'app-collection-status',
  imports: [AutoAnimateDirective, ButtonComponent, IconButtonComponent, MatIconModule],
  // The toast comes and goes as the host's child.
  hostDirectives: [AutoAnimateDirective],
  templateUrl: './collection-status.component.html',
  styleUrl: './collection-status.component.scss',
})
export class CollectionStatusComponent {
  private readonly data = inject(ActivityDataService)
  private readonly selection = inject(ActivitySelection)
  private readonly now = signal(Date.now())
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef)
  /** The pill or the card on screen, if any. */
  private readonly shown = viewChild<ElementRef<HTMLElement>>('shown')
  /** Its size, which changes with what it says, the window and the font coming in. */
  private readonly shownSize = elementSize(this.shown)
  private readonly viewport = windowSize()

  protected readonly elapsedSeconds = computed(() => {
    const status = this.data.collectionStatus()
    if (status.kind !== 'collecting') return 0
    return Math.max(0, Math.floor((this.now() - status.startedAt) / 1000))
  })

  protected readonly banner = computed<CollectionBannerVm | null>(() => {
    const status = this.data.collectionStatus()
    const { status: link, since } = this.data.connection()
    if (link.kind === 'reconnecting') {
      const stale = this.now() - since >= STALE_AFTER_MS
      return {
        tone: stale ? 'warn' : 'info',
        icon: 'cloud_sync',
        title: stale ? 'Reconnecting; the activity shown may be out of date' : 'Reconnecting',
        message: link.failure.message,
        busy: true,
        retry: false,
      }
    }
    switch (status.kind) {
      case 'idle':
        return null
      case 'collecting': {
        const providers = this.data.providers()
        const sources = providers.map(provider => provider.label)
        const pending = providers.filter(provider => provider.collecting).map(p => p.label)
        return {
          tone: 'info',
          icon: 'autorenew',
          title:
            status.operation === 'initial'
              ? 'Checking sources and collecting activity'
              : 'Collecting activity',
          message: pending.length
            ? `Still collecting ${pending.join(', ')}`
            : sources.length
              ? `From ${sources.join(', ')}`
              : 'Waiting for sources',
          busy: true,
          retry: false,
        }
      }
      case 'failed':
        return {
          tone: 'error',
          icon: 'error_outline',
          title: status.operation === 'initial' ? 'Could not load activity' : 'Collection failed',
          message: status.failure.message,
          busy: false,
          retry: retryable(status),
        }
    }
  })

  constructor() {
    const root = inject(DOCUMENT).documentElement
    afterRenderEffect({
      earlyRead: () => {
        const shown = this.shown()?.nativeElement
        this.shownSize()
        this.viewport.width()
        if (!shown) return null
        // The layout's place, not the drawn one: entering, the pill or card is still moving.
        const top = this.host.nativeElement.getBoundingClientRect().top + shown.offsetTop
        return Math.max(0, Math.ceil(this.viewport.height() - top))
      },
      write: clearance => {
        const value = clearance()
        if (value === null) root.style.removeProperty(STATUS_CLEARANCE)
        else root.style.setProperty(STATUS_CLEARANCE, `${value}px`)
      },
    })
    inject(DestroyRef).onDestroy(() => root.style.removeProperty(STATUS_CLEARANCE))

    // Ticks only while collecting or reconnecting: an interval of 0 stops it.
    interval(
      () => this.now.set(Date.now()),
      computed(() =>
        this.data.collectionStatus().kind === 'collecting' ||
        this.data.connection().status.kind === 'reconnecting'
          ? 1000
          : 0
      )
    )
  }

  /**
   * While nothing is collected, failed or reconnecting: the range the collection on screen covers,
   * where it leaves out part of this tab's range (another window collected another), and this
   * tab's, which Collect asks for.
   */
  protected readonly otherRange = computed<{ shown: string; own: string } | null>(() => {
    if (this.banner() !== null) return null
    const coverage = this.selection.rangeCoverage()
    if (coverage.kind !== 'other') return null
    return {
      shown: dayRangeLabel(coverage.collected.from, coverage.collected.to),
      own: dayRangeLabel(coverage.own.from, coverage.own.to),
    }
  })

  protected collectOwnRange(): void {
    void this.data.collectShownRange()
  }

  protected retry(): void {
    void this.data.retryCollection()
  }

  protected dismiss(): void {
    this.data.dismissCollectionStatus()
  }
}
