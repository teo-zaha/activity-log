import { InjectionToken, Service, computed, inject, linkedSignal, signal } from '@angular/core'
import type {
  ActivityEntry,
  ActivityLog,
  CollectedActivitySummary,
  ConnectionId,
  ProviderInfo,
} from '../generated/activity'
import { type ConnectionKey, connectionKey } from '../providers/connection'
import { createEntryAdapters, getProviderAdapter } from '../providers/provider-adapter'
import type { EntryUiAdapter } from '../providers/types'
import type { ActivityResponse } from './backend/backend-contract'
import { ActivityBackendClient } from './backend/backend-client'
import { extractBackendErrorMessage } from './backend/error-message'
import { ActivityViewStateService, type DayRange } from './activity-view-state.service'
import { dayKeyOf, lastDayKeyBefore, localDayRange } from '../utils/local-day'
import type { CollectionOperation, CollectionStatus, ProviderViewModel } from './types'

export const ACTIVITY_POLL_ATTEMPTS = new InjectionToken<number>('ACTIVITY_POLL_ATTEMPTS', {
  providedIn: 'root',
  // The backend gives up on a startup check after 15 s and on a collection after 120 s.
  factory: (): number => 180,
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
  /** The entries' adapters; an entry that a new answer repeats unchanged keeps its adapter. */
  readonly allEntryAdapters = linkedSignal<ActivityEntry[], EntryUiAdapter[]>({
    source: this.allEntries,
    computation: (entries, previous) => createEntryAdapters(entries, previous?.value ?? []),
  }).asReadonly()
  readonly providerInfo = signal<ProviderInfo[]>([])
  private readonly loadedProviders = signal(false)
  /** Whether the list of sources has come in once. */
  readonly providersLoaded = this.loadedProviders.asReadonly()
  private refreshingProviders = false
  /** Numbers the provider requests, so an answer older than the list on screen is dropped. */
  private providerRequests = 0
  private shownProviderRequest = 0
  /** Nothing is set up yet: the dashboard asks for a first source instead of showing views. */
  readonly hasNoSources = computed(() => this.providersLoaded() && this.providerInfo().length === 0)
  readonly summary = signal<CollectedActivitySummary | null>(null)
  readonly meta = signal<ActivityLog['meta'] | null>(null)
  readonly isLoading = signal(false)
  readonly error = signal<string | null>(null)
  readonly collectionStatus = signal<CollectionStatus>({ kind: 'idle' })

  readonly connectionCounts = computed<Map<ConnectionKey, number>>(
    () =>
      new Map(
        (this.summary()?.connections ?? []).map(({ connection, count }) => [
          connectionKey(connection),
          count,
        ])
      )
  )

  /** The connections the running collection still waits for. */
  readonly pendingConnections = computed<Set<ConnectionKey>>(
    () =>
      new Set(
        this.collectionStatus().kind === 'collecting'
          ? (this.meta()?.pending ?? []).map(connectionKey)
          : []
      )
  )

  private readonly providerViewModels = computed<ProviderViewModel[]>(() => {
    const pending = this.pendingConnections()
    return this.providerInfo().map(info => {
      const adapter = getProviderAdapter(info.connection.source)
      const key = connectionKey(info.connection)
      return {
        info,
        key,
        label: info.label,
        chipKey: info.connection.source,
        tooltip: adapter.getStatusTooltip(info),
        count: this.connectionCounts().get(key) ?? 0,
        collecting: pending.has(key),
      }
    })
  })

  /** Usable sources, and those whose startup check is still running. */
  readonly providers = computed(() =>
    this.providerViewModels().filter(p => p.info.usable || isChecking(p.info))
  )
  readonly unusableProviders = computed(() =>
    this.providerViewModels().filter(p => !p.info.usable && !isChecking(p.info))
  )

  /** Shows `log`; a partial one already holds what was shown for its pending connections. */
  loadLog(log: ActivityLog): void {
    this.allEntries.set(log.entries)
    this.summary.set(log.summary)
    this.meta.set(log.meta)
    this.showRange(log.meta)
  }

  /** Shows the backend's startup collection, or collects the range the dashboard was opened with. */
  async initLoad(): Promise<void> {
    const opened = this.viewState.openedRange()
    this.begin('initial')
    try {
      await this.loadProviders()
      await this.settle('initial', opened && (await this.collectOpenedRange(opened)))
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
      await this.settle('collect', await this.backend.collect(localDayRange(fromKey, toKey)))
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

  /**
   * Follows the collection the backend starts by itself after a settings change. Quieter than
   * `collect`: the selection stays, since the entries on screen stay until their source is
   * collected again, and a backend with nothing to show is not an error.
   */
  async followCollection(): Promise<void> {
    // A collection already being shown reads the same backend state.
    if (this.isLoading()) return
    this.begin('collect')
    try {
      await this.settle('collect', null)
    } catch {
      // Nothing was collected before the change, so nothing is collected again.
    } finally {
      this.finish()
    }
  }

  /**
   * Runs a source's startup check again. The backend then collects its kind again for the range
   * on screen, which is followed like the collection after a settings change.
   */
  async recheckConnection(connection: ConnectionId): Promise<void> {
    const { providers } = await this.backend.recheckConnection(connection)
    this.applyProviders(providers, false)
    await this.followCollection()
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

  loadProviders(): Promise<void> {
    return this.refreshProviders(true)
  }

  /**
   * Shows `providers`, a list newer than any request still on its way, such as the one a
   * settings change answers with.
   */
  applyProviders(providers: ProviderInfo[], activateAllUsable: boolean): void {
    this.shownProviderRequest = this.providerRequests
    this.showProviders(providers, activateAllUsable)
  }

  /** Asks for the sources and shows the answer, unless a newer list is on screen by then. */
  private async refreshProviders(activateAllUsable: boolean): Promise<void> {
    const request = ++this.providerRequests
    const { providers } = await this.backend.getProviders()
    if (request <= this.shownProviderRequest) return
    this.shownProviderRequest = request
    this.showProviders(providers, activateAllUsable)
  }

  private showProviders(providers: ProviderInfo[], activateAllUsable: boolean): void {
    const previous = this.providerInfo()
    this.providerInfo.set(providers)
    this.loadedProviders.set(true)
    if (activateAllUsable) {
      this.viewState.activateAllUsable(providers)
    } else {
      this.viewState.followProviders(providers, previous)
    }
    if (providers.some(isChecking)) void this.refreshWhileChecking()
  }

  /**
   * The backend runs the startup checks in the background and lists a source as checking until
   * its own check ends, so every answer is applied. Sources stay as the user switched them.
   */
  private async refreshWhileChecking(): Promise<void> {
    if (this.refreshingProviders) return
    this.refreshingProviders = true
    try {
      while (this.providerInfo().some(isChecking)) {
        await this.wait()
        await this.refreshProviders(false)
      }
    } catch {
      // A later save or reload asks again.
    } finally {
      this.refreshingProviders = false
    }
  }

  private wait(): Promise<void> {
    return new Promise(resolve => setTimeout(resolve, this.pollDelayMs))
  }

  /**
   * Shows a collection until it ends (`null` asks the backend right away). While it runs, each
   * answer is a partial log: it is shown when it brings something new, and the backend is asked
   * again. A failed collection keeps its range and reports the backend's error.
   */
  private async settle(
    operation: CollectionOperation,
    first: ActivityResponse | null
  ): Promise<void> {
    let response = first
    let fetches = 0
    let shown: string | null = null
    while (response === null || isRunning(response)) {
      if (response !== null && progressKey(response) !== shown) {
        this.loadLog(response)
        shown = progressKey(response)
      }
      if (fetches >= this.pollAttempts) {
        const message =
          operation === 'initial'
            ? 'Timed out waiting for initial collection.'
            : 'Timed out waiting for collection.'
        this.fail(operation, 'timedOut', message)
        return
      }
      if (response !== null) await this.wait()
      response = await this.backend.getActivity()
      fetches++
    }

    if ('error' in response) {
      this.showRange(response)
      this.fail(operation, 'failed', response.error)
      return
    }
    this.loadLog(response)
    // The log may name sources that are new here: the backend reloads a changed config file, and
    // the providers poll may be a second behind. The log stays if the sources cannot be read.
    await this.refreshProviders(false).catch(() => undefined)
  }

  /** Starts collecting `range`; the backend stops its startup collection of another range. */
  private collectOpenedRange({ from, to }: DayRange): Promise<ActivityResponse> {
    return this.backend.collect(localDayRange(from, to))
  }

  /** Shows a range of instants as the local days it covers. */
  private showRange({ start, end }: { start: string; end: string }): void {
    this.viewState.showCollectedRange({ from: dayKeyOf(start), to: lastDayKeyBefore(end) })
  }
}

function isChecking(info: ProviderInfo): boolean {
  return info.status === 'checking'
}

function isRunning(response: ActivityResponse): response is ActivityLog {
  return !('error' in response) && (response.meta.pending ?? []).length > 0
}

/** A running collection changes what it shows only when a connection settles. */
function progressKey({ meta }: ActivityLog): string {
  return [meta.start, meta.end, ...(meta.pending ?? []).map(connectionKey)].join('\u001f')
}
