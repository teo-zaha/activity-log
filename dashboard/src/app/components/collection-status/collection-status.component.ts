import { Component, computed, inject, signal } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { interval } from '@signality/core/scheduling/interval'
import { AutoAnimateDirective } from '../auto-animate/auto-animate.directive'
import { ButtonComponent } from '../button/button.component'
import { IconButtonComponent } from '../button/icon-button.component'
import { ActivityService } from '../../services/activity.service'
import { retryable } from '../../services/collection-failure'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'

type BannerTone = 'info' | 'warn' | 'error'

/** How long a reconnect keeps the state on screen before it counts as out of date. */
const STALE_AFTER_MS = 5000

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
  protected readonly activity = inject(ActivityService)
  private readonly settings = inject(SettingsDialogLauncher)
  private readonly now = signal(Date.now())

  protected readonly elapsedSeconds = computed(() => {
    const status = this.activity.collectionStatus()
    if (status.kind !== 'collecting') return 0
    return Math.max(0, Math.floor((this.now() - status.startedAt) / 1000))
  })

  protected readonly banner = computed<CollectionBannerVm | null>(() => {
    const status = this.activity.collectionStatus()
    const { status: link, since } = this.activity.connection()
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
        const providers = this.activity.providers()
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
      case 'timedOut':
        return {
          tone: 'warn',
          icon: 'timer_off',
          title: 'Collection timed out',
          message: `${status.message} A source may still be starting up or responding slowly.`,
          busy: false,
          retry: true,
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
    // Ticks only while collecting or reconnecting: an interval of 0 stops it.
    interval(
      () => this.now.set(Date.now()),
      computed(() =>
        this.activity.collectionStatus().kind === 'collecting' ||
        this.activity.connection().status.kind === 'reconnecting'
          ? 1000
          : 0
      )
    )
  }

  protected retry(): void {
    void this.activity.retryCollection()
  }

  protected dismiss(): void {
    this.activity.dismissCollectionStatus()
  }

  protected openSources(): void {
    this.settings.open(this.activity.unusableProviders()[0]?.key)
  }
}
