import { InjectionToken, Service, computed, inject, signal } from '@angular/core'
import type {
  ActivityEntry,
  ActivityLog,
  CollectedActivitySummary,
  CollectingResponse,
  ProviderInfo,
  Source,
} from '../generated/activity'
import { createEntryAdapter, getProviderAdapter } from '../providers/provider-adapter'
import type { ActivityResponse } from './backend/backend-contract'
import { ActivityBackendClient } from './backend/backend-client'
import { extractBackendErrorMessage } from './backend/error-message'
import { ActivityViewStateService } from './activity-view-state.service'
import { dayKeyOf, lastDayKeyBefore, localDayRange } from '../utils/local-day'
import type { CollectionOperation, CollectionStatus, ProviderViewModel } from './types'

export const ACTIVITY_POLL_ATTEMPTS = new InjectionToken<number>('ACTIVITY_POLL_ATTEMPTS', {
  providedIn: 'root',
  factory: (): number => 60,
})

export const ACTIVITY_POLL_DELAY_MS = new InjectionToken<number>('ACTIVITY_POLL_DELAY_MS', {
  providedIn: 'root',
  factory: (): number => 1000,
})

@Service()
export class ActivityDataService {
  private readonly backend = inject(ActivityBackendClient)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly pollAttempts = inject(ACTIVITY_POLL_ATTEMPTS)
  private readonly pollDelayMs = inject(ACTIVITY_POLL_DELAY_MS)

  readonly allEntries = signal<ActivityEntry[]>([])
  readonly allEntryAdapters = computed(() => this.allEntries().map(createEntryAdapter))
  readonly providerInfo = signal<ProviderInfo[]>([])
  readonly summary = signal<CollectedActivitySummary | null>(null)
  readonly meta = signal<ActivityLog['meta'] | null>(null)
  readonly isLoading = signal(false)
  readonly error = signal<string | null>(null)
  readonly collectionStatus = signal<CollectionStatus>({ kind: 'idle' })

  readonly sourceCounts = computed<Map<Source, number>>(() => {
    const summary = this.summary()
    if (!summary) return new Map()
    return new Map<Source, number>([
      ['git', summary.git_count],
      ['gitlab', summary.gitlab_count],
      ['jira', summary.jira_count],
      ['outlook', summary.outlook_count],
      ['system', summary.system_count],
    ])
  })

  private readonly providerViewModels = computed<ProviderViewModel[]>(() =>
    this.providerInfo().map(info => {
      const adapter = getProviderAdapter(info.id)
      return {
        info,
        label: adapter.label,
        icon: adapter.providerIcon,
        chipKey: info.id,
        tooltip: adapter.getStatusTooltip(info),
        count: this.sourceCounts().get(info.id) ?? 0,
      }
    })
  )

  readonly providers = computed(() => this.providerViewModels().filter(p => p.info.usable))
  readonly unusableProviders = computed(() => this.providerViewModels().filter(p => !p.info.usable))

  loadLog(log: ActivityLog): void {
    this.allEntries.set(log.entries)
    this.viewState.clearSelection()
    this.summary.set(log.summary)
    this.meta.set(log.meta)
    this.viewState.dateFrom.set(dayKeyOf(log.meta.start))
    this.viewState.dateTo.set(lastDayKeyBefore(log.meta.end))
  }

  async initLoad(): Promise<void> {
    this.begin('initial')
    try {
      await this.loadProviders()
      const log = await this.waitForActivity()
      if (log) {
        this.loadLog(log)
        return
      }
      this.fail('initial', 'timedOut', 'Timed out waiting for initial collection.')
    } catch (error) {
      this.fail('initial', 'failed', extractBackendErrorMessage(error))
    } finally {
      this.finish()
    }
  }

  /** Collects the local days `fromKey` to `toKey`, inclusive, in the browser's time zone. */
  async collect(fromKey: string, toKey: string): Promise<void> {
    this.begin('collect')
    try {
      const response = await this.backend.collect(localDayRange(fromKey, toKey))
      if (this.isCollectingResponse(response)) {
        const log = await this.waitForActivity()
        if (!log) {
          this.fail('collect', 'timedOut', 'Timed out waiting for collection.')
          return
        }
        this.loadLog(log)
        return
      }

      this.loadLog(response)
    } catch (error) {
      this.fail('collect', 'failed', extractBackendErrorMessage(error))
    } finally {
      this.finish()
    }
  }

  dismissCollectionStatus(): void {
    const status = this.collectionStatus()
    if (status.kind !== 'timedOut' && status.kind !== 'failed') return
    this.collectionStatus.set({ kind: 'idle' })
    if (this.error() === status.message) this.error.set(null)
  }

  private begin(operation: CollectionOperation): void {
    this.isLoading.set(true)
    this.error.set(null)
    this.collectionStatus.set({ kind: 'collecting', operation, startedAt: Date.now() })
  }

  private fail(operation: CollectionOperation, kind: 'timedOut' | 'failed', message: string): void {
    this.error.set(message)
    this.collectionStatus.set({ kind, operation, message })
  }

  private finish(): void {
    this.isLoading.set(false)
    if (this.collectionStatus().kind === 'collecting') {
      this.collectionStatus.set({ kind: 'idle' })
    }
  }

  async loadProviders(): Promise<void> {
    const response = await this.backend.getProviders()
    this.applyProviders(response.providers, true)
  }

  applyProviders(providers: ProviderInfo[], activateAllUsable: boolean): void {
    this.providerInfo.set(providers)
    if (activateAllUsable) {
      this.viewState.activateAllUsable(providers)
      return
    }

    this.viewState.retainUsableSources(providers)
  }

  private async waitForActivity(): Promise<ActivityLog | null> {
    for (let i = 0; i < this.pollAttempts; i++) {
      const response = await this.backend.getActivity()
      if (this.isCollectingResponse(response)) {
        await new Promise(resolve => setTimeout(resolve, this.pollDelayMs))
        continue
      }
      return response
    }

    return null
  }

  private isCollectingResponse(response: ActivityResponse): response is CollectingResponse {
    return 'collecting' in response
  }
}
