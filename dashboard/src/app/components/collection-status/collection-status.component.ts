import { Component, computed, effect, inject, signal } from '@angular/core'
import { ActivityService } from '../../services/activity.service'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'

type BannerTone = 'info' | 'warn' | 'error'

interface CollectionBannerVm {
  readonly tone: BannerTone
  readonly icon: string
  readonly title: string
  readonly message: string
  readonly busy: boolean
}

@Component({
  selector: 'app-collection-status',
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
        }
      }
      case 'timedOut':
        return {
          tone: 'warn',
          icon: 'timer_off',
          title: 'Collection timed out',
          message: `${status.message} A source may still be starting up or responding slowly.`,
          busy: false,
        }
      case 'failed':
        return {
          tone: 'error',
          icon: 'error_outline',
          title: status.operation === 'initial' ? 'Could not load activity' : 'Collection failed',
          message: status.message,
          busy: false,
        }
    }
  })

  constructor() {
    effect(onCleanup => {
      if (this.activity.collectionStatus().kind !== 'collecting') return
      this.now.set(Date.now())
      const timer = setInterval(() => this.now.set(Date.now()), 1000)
      onCleanup(() => clearInterval(timer))
    })
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
