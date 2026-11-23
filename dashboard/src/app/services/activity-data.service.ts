import { Service, computed, inject, linkedSignal, signal } from '@angular/core'
import type {
  ActivityEntry,
  ActivityLog,
  ActivityState,
  ActivityUpdate,
  CollectAnswer,
  CollectRequest,
  ConnectionId,
  InstallReceipt,
  ProviderConfigSaveResponseDto,
  ProviderInfo,
  ProvidersResponse,
  ProvidersUpdate,
  RecheckAnswer,
  RuntimeState,
} from '../generated/activity'
import { type ConnectionKey, connectionKey } from '../providers/connection'
import { createEntryAdapters, getProviderAdapter } from '../providers/provider-adapter'
import type { EntryUiAdapter } from '../providers/types'
import { ActivityBackendClient } from './backend/backend-client'
import { ActivityViewStateService, type DayRange } from './activity-view-state.service'
import { type Failure, failure, failureOf } from './failure'
import { BackendFailure } from './backend/backend-failure'
import {
  RuntimeStateSource,
  type SourceFailure,
  type SourceStatus,
} from './runtime-state/runtime-state-source'
import { dayKeyOf, lastDayKeyBefore, localDayRange, type DayKey } from '../utils/local-day'
import type {
  CollectionOperation,
  CollectionStatus,
  ProviderViewModel,
  ReceiptStanding,
} from './types'

/**
 * The host's state on screen. A stream starts with both parts, but a command's answer can come
 * before the stream's first pair, so either part can still be missing at first.
 */
interface Installed {
  readonly epoch: string
  readonly authority: string
  readonly activity: ActivityUpdate | null
  readonly providers: ProvidersUpdate | null
  /**
   * A pair of this host was taken. Until then the host came from a command's answer alone, and
   * a first pair of another host only asks for a fresh handshake.
   */
  readonly confirmed: boolean
}

/**
 * The latest collect request, on its way or refused, and the host it was sent to (`epoch`; one
 * sent before any host is bound to the first installed): a newer command, or another host, ends
 * its say.
 */
type CollectCommand =
  | {
      readonly token: number
      readonly epoch: string | null
      readonly operation: CollectionOperation
      readonly phase: 'sending'
    }
  | {
      readonly token: number
      readonly epoch: string | null
      readonly operation: CollectionOperation
      readonly phase: 'refused'
      readonly failure: Failure
    }

/** How the source reaches the backend; `episode` counts its failures, so each is dismissed alone. */
export interface SourceLink {
  readonly status: SourceStatus
  /** When the status began, for how long a reconnect has kept the state on screen. */
  readonly since: number
  readonly episode: number
}

/** Who a failure shown in the shell belongs to, and which one it is. */
interface Outcome {
  readonly origin: 'command' | 'source' | 'state'
  /** The command's token, the source's failure episode or the activity revision. */
  readonly id: number
  readonly status: Extract<CollectionStatus, { kind: 'failed' }>
}

type OutcomeKey = Pick<Outcome, 'origin' | 'id'>

/**
 * A part of an authority not installed came past everything installed, so a pair was missed:
 * the highest such revision, and whether a new attempt was asked for and has not brought its
 * first pair yet.
 */
interface Resync {
  readonly evidence: number
  readonly outstanding: boolean
}

const NO_ENTRIES: ActivityEntry[] = []
const NO_PROVIDERS: ProviderInfo[] = []
const NO_RESYNC: Resync = { evidence: 0, outstanding: false }

/**
 * The collection and the sources on screen, as the host has them. A `RuntimeStateSource`
 * delivers the host's stamped state; what is installed is decided by the stamps alone
 * (`docs/plans/shared-contracts.md` §5, coordinator rules) and everything shown is derived from
 * it. Commands send; each takes a token when it is sent, and only the latest command's answer
 * is installed, by its stamp. A save's receipt is kept until its revision is installed.
 */
@Service()
export class ActivityDataService {
  private readonly backend = inject(ActivityBackendClient)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly source = inject(RuntimeStateSource)

  private readonly installed = signal<Installed | null>(null)
  private readonly connected = signal(false)
  private readonly link = signal<SourceLink>({
    status: { kind: 'connecting' },
    since: 0,
    episode: 0,
  })
  private readonly collectCommand = signal<CollectCommand | null>(null)
  private readonly operation = signal<CollectionOperation>('initial')
  private readonly dismissed = signal<readonly OutcomeKey[]>([])
  /**
   * The receipts of saves whose sources show every usable one once installed. All of the host
   * installed: a receipt of another is never kept, and another host's install drops them.
   */
  private pendingActivations: readonly InstallReceipt[] = []
  private resync = NO_RESYNC
  private latestToken = 0
  /**
   * The range the old host showed, which the URL may have followed, from a restart until the new
   * host shows a range of its own; none where a collect was on its way, whose range decides.
   */
  private restartedFrom: DayRange | null = null
  private starting: Promise<void> | null = null

  private readonly activityState = computed<ActivityState | null>(
    () => this.installed()?.activity?.state ?? null
  )
  private readonly log = computed<ActivityLog | null>(() => {
    const state = this.activityState()
    return state?.kind === 'running' || state?.kind === 'ready' ? state.log : null
  })
  readonly allEntries = computed(() => this.log()?.entries ?? NO_ENTRIES)
  /** The entries' adapters; an entry that a new answer repeats unchanged keeps its adapter. */
  readonly allEntryAdapters = linkedSignal<ActivityEntry[], EntryUiAdapter[]>({
    source: this.allEntries,
    computation: (entries, previous) => createEntryAdapters(entries, previous?.value ?? []),
  }).asReadonly()
  readonly summary = computed(() => this.log()?.summary ?? null)
  readonly meta = computed(() => this.log()?.meta ?? null)
  readonly providerInfo = computed(
    () => this.installed()?.providers?.providers.providers ?? NO_PROVIDERS
  )
  /** Where new secrets are sealed here; `null` until the sources came in. */
  readonly secretStore = computed(() => this.installed()?.providers?.providers.secretStore ?? null)
  /** Whether the list of sources has come in once. */
  readonly providersLoaded = computed(() => (this.installed()?.providers ?? null) !== null)
  /** Nothing is set up yet: the dashboard asks for a first source instead of showing views. */
  readonly hasNoSources = computed(() => this.providersLoaded() && this.providerInfo().length === 0)
  /** How the dashboard reaches the backend, for the view to show a reconnect over the state. */
  readonly connection = this.link.asReadonly()

  /**
   * A collection is asked for or runs, or the first one is still on its way. A lost backend ends
   * it, also while a request waits for its answer.
   */
  private readonly collecting = computed(() => {
    if (!this.connected() || this.link().status.kind === 'failed') return false
    if (this.collectCommand()?.phase === 'sending') return true
    const state = this.activityState()
    if (state === null || state.kind === 'empty') return !this.hasNoSources()
    return state.kind === 'running'
  })
  readonly isLoading = this.collecting
  /**
   * When the collection on screen began: the dashboard's start until a first state, then each run
   * boundary as it is accepted (`markRun`): a collect sent, another host, a run that follows one
   * that ended, a run of another range. Updates of one run keep it. Set where each is accepted,
   * not derived from `collecting`, which only tells whether one runs: a run that ends and the
   * next that begins between two reads look alike there (B77).
   */
  private readonly runStartedAt = signal(Date.now())

  /** What failed, most direct first: a refused request, a lost backend, a failed collection. */
  private readonly outcomes = computed<readonly Outcome[]>(() => {
    const outcomes: Outcome[] = []
    const command = this.collectCommand()
    if (command?.phase === 'refused') {
      outcomes.push({
        origin: 'command',
        id: command.token,
        status: {
          kind: 'failed',
          operation: command.operation,
          origin: 'collect',
          failure: command.failure,
        },
      })
    }
    const { status, episode } = this.link()
    if (status.kind === 'failed') {
      outcomes.push({ origin: 'source', id: episode, status: this.sourceOutcome(status.cause) })
    }
    const activity = this.installed()?.activity
    if (activity && activity.state.kind === 'failed') {
      outcomes.push({
        origin: 'state',
        id: activity.revision,
        status: {
          kind: 'failed',
          operation: this.operation(),
          origin: 'collection',
          failure: failure(activity.state.failure.error),
        },
      })
    }
    return outcomes
  })
  private readonly shownOutcome = computed<Outcome | null>(() => {
    const dismissed = this.dismissed()
    return (
      this.outcomes().find(
        outcome => !dismissed.some(key => key.origin === outcome.origin && key.id === outcome.id)
      ) ?? null
    )
  })

  readonly collectionStatus = computed<CollectionStatus>(() => {
    if (this.collecting()) {
      return { kind: 'collecting', operation: this.operation(), startedAt: this.runStartedAt() }
    }
    return this.shownOutcome()?.status ?? { kind: 'idle' }
  })

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
  /** Whether any source is usable or still being checked: whether a view can show activity. */
  readonly anySourceReady = computed(() => this.providers().length > 0)
  readonly unusableProviders = computed(() =>
    this.providerViewModels().filter(p => !p.info.usable && !isChecking(p.info))
  )

  /**
   * Shows the host's state from now on, once, and collects the range the dashboard was opened
   * with. Resolves once nothing is followed any more.
   */
  initLoad(): Promise<void> {
    return (this.starting ??= this.start())
  }

  /**
   * Collects the local days `fromKey` to `toKey`, inclusive, in the browser's time zone.
   * Resolves once the backend has answered; the collection is followed on its own.
   */
  async collect(fromKey: DayKey, toKey: DayKey): Promise<void> {
    this.connect()
    await this.sendCollect('collect', localDayRange(fromKey, toKey))
  }

  /**
   * Collects the range on screen (Collect, ⌘R): this tab's own, else the collected one. Null when
   * there is none to collect, and nothing is sent.
   */
  collectShownRange(): Promise<void> | null {
    const [from, to] = [this.viewState.dateFrom(), this.viewState.dateTo()]
    return from !== null && to !== null ? this.collect(from, to) : null
  }

  /**
   * Makes `range` this tab's own, the URL's `from`/`to`, and collects it, as the range form's
   * Collect does with the dates it holds: an empty view's Collect.
   */
  async collectRange(range: DayRange): Promise<void> {
    await this.viewState.setRange(range.from, range.to)
    await this.collectShownRange()
  }

  /**
   * Runs a source's startup check again; the backend then collects its kind again for the range
   * on screen, which is followed on its own. Resolves once the backend has answered; a refused
   * request rejects, for its caller to show.
   */
  async recheckConnection(connection: ConnectionId): Promise<void> {
    this.connect()
    const token = this.dispatch('collect')
    const answer = await this.answered(
      this.answeredWithin(this.backend.recheckConnection(connection))
    )
    this.accept(token, () => this.applyProviders(providersUpdateOf(answer)))
  }

  /**
   * Sends a settings change. Every usable source shows once the change's install is on screen,
   * whatever other command came meanwhile; the collection the backend starts for it is followed.
   * Quieter than `collect`: the entries on screen stay until their source is collected again.
   */
  async saveSettings<Response extends ProviderConfigSaveResponseDto>(
    request: () => Promise<Response>
  ): Promise<Response> {
    this.connect()
    this.dispatch('collect')
    const response = await this.answered(request())
    this.awaitInstall(response.installed)
    this.source.follow()
    return response
  }

  /**
   * Where a save's receipt stands against what is on screen: of this host and nothing later
   * installed (or no host yet to tell), of this host with a later providers revision installed,
   * of another host, or none (written, never installed).
   */
  receiptStanding(receipt: InstallReceipt | null): ReceiptStanding {
    if (receipt === null) return 'notInstalled'
    const installed = this.installed()
    if (installed === null) return 'current'
    if (installed.epoch !== receipt.epoch) return 'otherHost'
    return receipt.revision >= (installed.providers?.revision ?? 0) ? 'current' : 'superseded'
  }

  /** The host on screen, null before any. */
  readonly hostEpoch = computed(() => this.installed()?.epoch ?? null)

  /**
   * Retry for the failure shown: a lost backend is reconnected; a refused or failed collection
   * of a known range is asked for again.
   */
  async retryCollection(): Promise<void> {
    const from = this.viewState.dateFrom()
    const to = this.viewState.dateTo()
    if (this.shownOutcome()?.origin === 'source' || !from || !to) {
      this.connect()
      this.source.retry()
      return this.source.settled()
    }
    return this.collect(from, to)
  }

  dismissCollectionStatus(): void {
    const shown = this.shownOutcome()
    if (shown === null) return
    // Only failures still shown can come back; the others are forgotten.
    const current = this.outcomes()
    this.dismissed.update(keys => [
      ...keys.filter(key => current.some(o => o.origin === key.origin && o.id === key.id)),
      { origin: shown.origin, id: shown.id },
    ])
  }

  private async start(): Promise<void> {
    const opened = this.viewState.openedRange()
    this.connect()
    if (opened) await this.sendCollect('initial', localDayRange(opened.from, opened.to))
    await this.source.settled()
  }

  /** Follows the host's state from now on; once. */
  private connect(): void {
    if (this.connected()) return
    this.connected.set(true)
    this.source.connect({
      state: (pair, authoritative) => this.applyPair(pair, authoritative),
      activity: update => this.applyActivity(update),
      providers: update => this.applyProviders(update),
      connection: status => this.applyStatus(status),
    })
  }

  private async sendCollect(operation: CollectionOperation, range: CollectRequest): Promise<void> {
    const token = this.dispatch(operation)
    const epoch = this.installed()?.epoch ?? null
    if (!this.collecting()) this.runStartedAt.set(Date.now())
    this.restartedFrom = null
    this.collectCommand.set({ token, epoch, operation, phase: 'sending' })
    try {
      const answer = await this.answered(this.answeredWithin(this.backend.collect(range)))
      if (this.collectCommand()?.token === token) this.collectCommand.set(null)
      this.accept(token, () => this.applyActivity(activityUpdateOf(answer)))
    } catch (error) {
      const command = this.collectCommand()
      if (command?.token === token) {
        // The host it was bound to meanwhile, if any.
        const { epoch } = command
        this.collectCommand.set({
          token,
          epoch,
          operation,
          phase: 'refused',
          failure: failureOf(error),
        })
      }
    }
  }

  /**
   * `answer`, or a failure once it took longer than `COMMAND_ANSWER_TIMEOUT_MS`. The command may
   * still reach the host after that: what it did is pushed, and when its answer comes after all,
   * either way, the source is told the backend answered; the answer itself has no say any more.
   */
  private answeredWithin<T>(answer: Promise<T>): Promise<T> {
    return new Promise((resolve, reject) => {
      let lost = false
      // A plain one-shot timer per command, which no Signality composable covers.
      const timer = setTimeout(() => {
        lost = true
        reject(
          new BackendFailure(
            'unreachable',
            `The backend did not answer within ${COMMAND_ANSWER_TIMEOUT_MS / 1000} s`
          )
        )
      }, COMMAND_ANSWER_TIMEOUT_MS)
      const settle = (done: () => void): void => {
        clearTimeout(timer)
        if (lost) this.source.follow()
        else done()
      }
      answer.then(
        value => settle(() => resolve(value)),
        (error: unknown) => settle(() => reject(error))
      )
    })
  }

  /**
   * A command's answer. Its failure may come after the host applied it (a lost or late answer),
   * whose result the host pushes; the source is told either way, and the command is never sent
   * again.
   */
  private async answered<T>(answer: Promise<T>): Promise<T> {
    try {
      return await answer
    } catch (error) {
      this.source.follow()
      throw error
    }
  }

  /**
   * Takes the next token for a command being sent: a collect request still on its way, or
   * refused, no longer has a say.
   */
  private dispatch(operation: CollectionOperation): number {
    this.operation.set(operation)
    this.collectCommand.set(null)
    return ++this.latestToken
  }

  /**
   * The backend accepted a command: the latest one's answer is installed by its stamp; an older
   * one's is not, since a newer command was sent after it. Either way the source is told the
   * backend answered (a source that gave up connects again).
   */
  private accept(token: number, install: () => void): void {
    if (token === this.latestToken) install()
    this.source.follow()
  }

  private applyPair(pair: RuntimeState, authoritative: boolean): void {
    const installed = this.installed()
    const { epoch, authority } = pair.activity
    if (installed === null) {
      this.install(pair.activity, pair.providers, true)
    } else if (epoch !== installed.epoch) {
      // Another host may come only with an attempt's first pair, and over a host that only a
      // command's answer showed (an older pair may still bring the host before) only with the
      // first pair of a handshake asked for here. Otherwise one is asked for.
      if (!authoritative || (!installed.confirmed && !this.resync.outstanding)) {
        return this.resyncFor(0)
      }
      this.install(pair.activity, pair.providers, true)
    } else if (authority === installed.authority) {
      if (!installed.confirmed) this.installed.set({ ...installed, confirmed: true })
      this.applyActivity(pair.activity)
      this.applyProviders(pair.providers)
    } else if (Math.max(pair.activity.revision, pair.providers.revision) > watermark(installed)) {
      this.install(pair.activity, pair.providers, true)
    }
    if (authoritative) this.reconsiderResync()
  }

  /**
   * Installs a host's state whole. Another host (another epoch, or the first) drops what belonged
   * to the old one: dismissed failures, a refusal, receipts, missed evidence, and the sources the
   * user hid.
   */
  private install(
    activity: ActivityUpdate | null,
    providers: ProvidersUpdate | null,
    confirmed: boolean
  ): void {
    const stamp = activity ?? providers
    if (stamp === null) return
    const previous = this.installed()
    const newHost = previous === null || previous.epoch !== stamp.epoch
    if (newHost && previous !== null) {
      this.runStartedAt.set(Date.now())
      const followed = previous.activity && dayRangeOf(previous.activity.state)
      this.restartedFrom =
        this.collectCommand()?.phase === 'sending' ? null : (this.restartedFrom ?? followed)
    } else if (activity) {
      this.markRun(previous?.activity ?? null, activity)
    }
    this.installed.set({
      epoch: stamp.epoch,
      authority: stamp.authority,
      activity,
      providers,
      confirmed: confirmed || (!newHost && previous?.confirmed === true),
    })
    if (newHost) {
      this.dismissed.set([])
      this.resync = NO_RESYNC
      this.pendingActivations = this.pendingActivations.filter(r => r.epoch === stamp.epoch)
      // A collect sent before any host belongs to the first; one sent to another host is over.
      const command = this.collectCommand()
      if (command?.epoch === null && previous === null) {
        this.collectCommand.set({ ...command, epoch: stamp.epoch })
      } else if (command !== null && command.epoch !== stamp.epoch) {
        this.collectCommand.set(null)
      }
    }
    if (activity) this.showRange(activity.state)
    if (providers) this.showProviders(providers, newHost ? null : (previous?.providers ?? null))
  }

  private applyActivity(update: ActivityUpdate): void {
    const installed = this.installed()
    if (installed === null) return this.install(update, null, false)
    if (!this.current(installed, update)) return
    if (installed.activity && update.revision <= installed.activity.revision) return
    this.markRun(installed.activity, update)
    this.installed.set({ ...installed, activity: update })
    this.showRange(update.state)
  }

  private applyProviders(update: ProvidersUpdate): void {
    const installed = this.installed()
    if (installed === null) return this.install(null, update, false)
    if (!this.current(installed, update)) return
    if (installed.providers && update.revision <= installed.providers.revision) return
    this.installed.set({ ...installed, providers: update })
    this.showProviders(update, installed.providers)
  }

  /** Starts the run's time at a run that begins with `next`: after none running, or of another range. */
  private markRun(previous: ActivityUpdate | null, next: ActivityUpdate): void {
    if (next.state.kind !== 'running') return
    const before = previous?.state.kind === 'running' ? rangeOf(previous.state) : null
    const after = rangeOf(next.state)
    if (
      before === null ||
      after === null ||
      before.start !== after.start ||
      before.end !== after.end
    ) {
      this.runStartedAt.set(Date.now())
    }
  }

  /**
   * Whether an update belongs to what is installed. One of the same host under an authority not
   * installed, past everything installed, means a pair was missed: the source starts a new
   * attempt, one at a time.
   */
  private current(installed: Installed, update: ActivityUpdate | ProvidersUpdate): boolean {
    if (update.epoch !== installed.epoch) return false
    if (update.authority === installed.authority) return true
    if (update.revision > watermark(installed)) this.resyncFor(update.revision)
    return false
  }

  /** Asks the source for a new attempt, unless one is out, and keeps `revision` as evidence. */
  private resyncFor(revision: number): void {
    if (!this.resync.outstanding) this.source.retry()
    this.resync = { evidence: Math.max(this.resync.evidence, revision), outstanding: true }
  }

  /** An attempt's first pair is in: it answers the resync, or one more is needed. */
  private reconsiderResync(): void {
    const installed = this.installed()
    if (installed !== null && this.resync.evidence > watermark(installed)) {
      this.resync = { evidence: this.resync.evidence, outstanding: true }
      this.source.retry()
    } else {
      this.resync = NO_RESYNC
    }
  }

  /**
   * Moves the sources shown from `previous` to `update`, keeping the user's choices; the first
   * list of a host shows every usable source.
   */
  private showProviders(update: ProvidersUpdate, previous: ProvidersUpdate | null): void {
    const providers = update.providers.providers
    if (previous === null) {
      this.viewState.activateAllUsable(providers)
    } else {
      this.viewState.followProviders(providers, previous.providers.providers)
    }
    this.activatePending()
  }

  /**
   * Keeps a save's receipt until its revision is installed; another host's never will be, nor a
   * save without one.
   */
  private awaitInstall(receipt: InstallReceipt | null): void {
    if (receipt === null || this.receiptStanding(receipt) === 'otherHost') return
    this.pendingActivations = [...this.pendingActivations, receipt]
    this.activatePending()
  }

  private activatePending(): void {
    const providers = this.installed()?.providers
    if (!providers) return
    const reached = this.pendingActivations.filter(
      receipt => receipt.revision <= providers.revision
    )
    if (reached.length === 0) return
    this.pendingActivations = this.pendingActivations.filter(r => !reached.includes(r))
    this.viewState.activateAllUsable(providers.providers.providers)
  }

  private applyStatus(status: SourceStatus): void {
    const link = this.link()
    if (status.kind === link.status.kind && status.kind !== 'reconnecting') return
    this.link.set({
      status,
      since: status.kind === link.status.kind ? link.since : Date.now(),
      episode: link.episode + (status.kind === 'failed' ? 1 : 0),
    })
  }

  private sourceOutcome(cause: SourceFailure): Outcome['status'] {
    const operation = this.operation()
    switch (cause.kind) {
      case 'unreachable':
        return { kind: 'failed', operation, origin: 'lostBackend', failure: cause.failure }
      case 'refused':
        return { kind: 'failed', operation, origin: 'refusedRead', failure: cause.failure }
    }
  }

  /**
   * Shows the local days the collection covers; nothing collected shows none. A restarted host's
   * first range goes to the URL's range that followed the old host's.
   */
  private showRange(state: ActivityState): void {
    const range = dayRangeOf(state)
    this.viewState.showCollectedRange(range)
    if (range === null || this.restartedFrom === null) return
    this.viewState.followRestartedHost(this.restartedFrom, range)
    this.restartedFrom = null
  }
}

function isChecking(info: ProviderInfo): boolean {
  return info.status === 'checking'
}

/** How long a command's answer may take before it counts as lost. */
const COMMAND_ANSWER_TIMEOUT_MS = 30_000

function watermark(installed: Installed): number {
  return Math.max(installed.activity?.revision ?? 0, installed.providers?.revision ?? 0)
}

/** The local days a collection covers, none for nothing collected. */
function dayRangeOf(state: ActivityState): DayRange | null {
  const range = rangeOf(state)
  return range && { from: dayKeyOf(range.start), to: lastDayKeyBefore(range.end) }
}

function rangeOf(state: ActivityState): { start: string; end: string } | null {
  switch (state.kind) {
    case 'empty':
      return null
    case 'failed':
      return state.failure
    case 'running':
    case 'ready':
      return state.log.meta
  }
}

/** A collect's answer as the update of the activity it installed, of the kind the host says. */
function activityUpdateOf(answer: CollectAnswer): ActivityUpdate {
  switch (answer.kind) {
    case 'failed': {
      const { start, end, error } = answer
      return { ...answer.stamp, state: { kind: 'failed', failure: { start, end, error } } }
    }
    case 'running':
    case 'ready': {
      const { meta, summary, entries } = answer
      return { ...answer.stamp, state: { kind: answer.kind, log: { meta, summary, entries } } }
    }
  }
}

/** A recheck's answer as the update of the sources it installed. */
function providersUpdateOf(answer: RecheckAnswer): ProvidersUpdate {
  const providers: ProvidersResponse = {
    currentOs: answer.currentOs,
    providers: answer.providers,
    secretStore: answer.secretStore,
  }
  return { ...answer.stamp, providers }
}
