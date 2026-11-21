import {
  Component,
  computed,
  effect,
  inject,
  input,
  linkedSignal,
  output,
  resource,
  signal,
  untracked,
} from '@angular/core'
import { applyEach, disabled, form, FormField, FormRoot, validate } from '@angular/forms/signals'
import { MatDialog } from '@angular/material/dialog'
import { MatFormFieldModule } from '@angular/material/form-field'
import { MatInputModule } from '@angular/material/input'
import { MatSelectModule } from '@angular/material/select'
import { MatIconModule } from '@angular/material/icon'
import { AutoAnimateDirective } from '../../auto-animate/auto-animate.directive'
import { FeedbackComponent } from '../../feedback/feedback.component'
import { IconButtonComponent } from '../../button/icon-button.component'
import { ButtonComponent } from '../../button/button.component'
import type {
  ProviderConfigFieldIssueDto,
  ProviderConfigFieldRefDto,
  ProviderConfigFieldStateDto,
  ProviderConfigSaveRequestDto,
} from '../../../generated/activity'
import { type ConnectionKey, connectionKey } from '../../../providers/connection'
import { ActivityDataService } from '../../../services/activity-data.service'
import { ConnectionSettings } from '../../../services/connection-settings.service'
import { type Failure, FailureError, failureOf, refusedAs } from '../../../services/failure'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
import { ToastService } from '../../../services/toast.service'
import { confirmAction } from '../../confirm-dialog/confirm-dialog.component'
import type { SettingsProviderRow } from '../settings-dialog.types'
import { SourceDrafts } from './source-drafts.service'
import { SourceHealthComponent } from './source-health/source-health.component'
import { RemovedSourceDraftComponent } from '../removed-source-draft/removed-source-draft.component'
import { SourceOperations } from './source-operations'
import {
  type DraftForm,
  type DraftValues,
  type ListDrafts,
  type ListField,
  type SingleField,
  type SourceDraft,
  draftOf,
  formOf,
  sameDraft,
  saveRequestOf,
} from './source-draft-request'

/** How a check of a draft ended: what a save of it would be refused for, or why it failed. */
type CheckResult =
  | { readonly kind: 'issues'; readonly issues: readonly ProviderConfigFieldIssueDto[] }
  | { readonly kind: 'failed'; readonly failure: Failure }

/**
 * A draft another program got in before, and where its recovery stands: the snapshot is read
 * again (`reading`), then the draft's request, with its own `basedOn`, is checked against it and
 * shown with what changed (`shown`, `revision` naming the snapshot now on screen). Blur, save and
 * removal all begin it the same way.
 */
type Recovery =
  | {
      readonly phase: 'reading'
      readonly key: ConnectionKey
      readonly draft: SourceDraft
      readonly request: ProviderConfigSaveRequestDto
    }
  | {
      readonly phase: 'shown'
      readonly key: ConnectionKey
      readonly draft: SourceDraft
      readonly issues: readonly ProviderConfigFieldIssueDto[]
      readonly revision: string
    }

/** A draft the backend checked, and how the check ended: both belong to that draft only. */
interface CheckedDraft {
  readonly key: ConnectionKey
  readonly draft: SourceDraft
  readonly result: CheckResult
}

@Component({
  selector: 'app-settings-source-pane',
  imports: [
    SourceHealthComponent,
    RemovedSourceDraftComponent,
    AutoAnimateDirective,
    FeedbackComponent,
    IconButtonComponent,
    ButtonComponent,
    MatIconModule,
    FormField,
    FormRoot,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
  ],
  templateUrl: './settings-source-pane.component.html',
  styleUrl: './settings-source-pane.component.scss',
})
export class SettingsSourcePaneComponent {
  private readonly data = inject(ActivityDataService)
  private readonly settings = inject(ConnectionSettings)
  private readonly runtime = inject(RuntimeEnvironmentService)
  private readonly dialog = inject(MatDialog)
  private readonly toast = inject(ToastService)
  private readonly drafts = inject(SourceDrafts)

  readonly provider = input.required<SettingsProviderRow>()
  /** Emitted once the connection was removed, or its draft discarded after another program did. */
  readonly removed = output<void>()
  /** A connection to open instead: the source added again for a draft whose source was removed. */
  readonly opened = output<ConnectionKey>()

  protected readonly isDesktop = this.runtime.isTauri()

  private readonly selectionKey = computed(() => this.provider().key)
  /**
   * A save, a removal or a check runs for the connection it was started on, also once another one
   * is shown: its answer, and its failure, belong to that connection.
   */
  private readonly operations = new SourceOperations()
  protected readonly savingConfig = computed(() =>
    this.operations.isRunning(this.selectionKey(), 'save')
  )

  /** Snapshot reads that came back, so a save answered after one does not replace it. */
  private snapshotReads = 0
  /**
   * The snapshot holds every connection; it reloads whenever another one is selected, and a read
   * for one no longer selected is dropped. A read that comes back, or fails, under another host
   * than it was sent under is sent again under the one on screen.
   */
  private readonly config = resource({
    params: () => (this.isDesktop ? this.selectionKey() : undefined),
    loader: async () => {
      for (;;) {
        const host = untracked(this.data.hostEpoch)
        try {
          const snapshot = await this.settings.loadProviderConfig()
          if (untracked(this.data.hostEpoch) === host) {
            this.snapshotReads++
            return snapshot
          }
        } catch (error) {
          if (untracked(this.data.hostEpoch) === host) {
            throw new FailureError(failureOf(error, 'Failed to load provider settings.'))
          }
        }
      }
    },
  })

  protected readonly loadingConfig = this.config.isLoading

  /** This connection's settings, once loaded. */
  private readonly connectionState = computed(() => {
    // `value()` throws while the resource is in its error state.
    if (!this.config.hasValue()) {
      return null
    }

    const key = this.selectionKey()
    return (
      this.config
        .value()
        .connections.find(connection => connectionKey(connection.connection) === key) ?? null
    )
  })

  protected readonly configFields = computed(() => this.connectionState()?.fields ?? [])
  protected readonly hasConnectionState = computed(() => this.connectionState() !== null)

  /** Unsaved edits left earlier, applied when this connection's settings load. */
  private readonly savedDraft = computed<SourceDraft | undefined>(() => {
    const key = this.selectionKey()
    return this.connectionState() ? untracked(() => this.drafts.get(key)) : undefined
  })

  /** The fields as the snapshot holds them, with the saved draft's edits over them. */
  private readonly loadedForm = computed<DraftForm | null>(() => {
    const state = this.connectionState()
    return state && formOf(state, this.savedDraft())
  })

  /** The label the user is typing. */
  protected readonly labelDraft = linkedSignal(() => this.loadedForm()?.label ?? '')
  protected readonly labelForm = form(this.labelDraft, path => {
    disabled(path, { when: () => this.savingConfig() })
    validate(path, () => this.issueAt({ kind: 'label' }))
  })
  protected readonly removing = computed(() =>
    this.operations.isRunning(this.selectionKey(), 'remove')
  )

  private readonly draftValues = linkedSignal<DraftValues>(() => ({
    ...this.loadedForm()?.values,
  }))
  protected readonly listDrafts = linkedSignal<ListDrafts>(() => ({
    ...this.loadedForm()?.lists,
  }))
  /** List keys the user reset, so their default applies again after saving. */
  private readonly resetLists = linkedSignal<ReadonlySet<string>>(
    () => new Set(this.loadedForm()?.resets)
  )

  /** An item shows the issue the backend reported for its place among the non-blank items. */
  protected readonly listForm = form(this.listDrafts, path => {
    applyEach(path, list => {
      applyEach(list, item => {
        disabled(item, { when: () => this.savingConfig() })
        // `pathKeys` is `[fieldKey, index]`.
        validate(item, ({ value, pathKeys }) => {
          const [key = '', row = ''] = pathKeys()
          if (value().trim() === '') return undefined
          const items = this.listDrafts()[key] ?? []
          const index = items.slice(0, Number(row)).filter(text => text.trim() !== '').length
          return this.issueAt({ kind: 'item', key, index })
        })
      })
    })
  })

  protected readonly configError = computed(() => {
    const key = this.selectionKey()
    const failure =
      this.operations.failure(key, 'save') ??
      this.operations.failure(key, 'remove') ??
      this.checkFailure() ??
      loadFailureOf(this.config.error())
    return failure?.message ?? null
  })

  /** From the press until the backend has accepted the new check. */
  protected readonly rechecking = computed(() =>
    this.operations.isRunning(this.selectionKey(), 'recheck')
  )
  protected readonly recheckError = computed(
    () => this.operations.failure(this.selectionKey(), 'recheck')?.message ?? null
  )

  protected async recheck(): Promise<void> {
    const { key, info } = this.provider()
    await this.operations.run(key, 'recheck', 'Checking the source again failed.', () =>
      this.data.recheckConnection(info.connection)
    )
  }

  /** What the user changed, against the settings on screen; null before they are loaded. */
  /** The revision of the snapshot on screen, which a removal is based on. */
  private readonly snapshotRevision = computed(() =>
    this.config.hasValue() ? this.config.value().revision : null
  )

  /**
   * The snapshot the edits were made against: a stored draft's own, else the one on screen when
   * editing began. "Keep my changes" after a conflict moves it to the snapshot on screen.
   */
  private readonly draftBase = linkedSignal<string | null>(() =>
    this.connectionState() ? (this.savedDraft()?.basedOn ?? this.snapshotRevision()) : null
  )

  private readonly draft = computed<SourceDraft | null>(() => {
    const state = this.connectionState()
    const basedOn = this.draftBase()
    if (state === null || basedOn === null) return null
    const form = {
      label: this.labelDraft(),
      values: this.draftValues(),
      lists: this.listDrafts(),
      resets: this.resetLists(),
    }
    return draftOf(state, form, basedOn)
  })

  /** The edits as one save request for this connection, or null when nothing changed. */
  protected readonly pendingRequest = computed<ProviderConfigSaveRequestDto | null>(() => {
    const state = this.connectionState()
    const draft = this.draft()
    return state && draft && saveRequestOf(state, draft)
  })

  /**
   * The last check the backend answered, with the draft it checked. Values are checked by the
   * backend alone (`check_provider_config`), and only the ones the draft changes, so a stored
   * value that no longer fits never blocks saving.
   */
  private readonly checked = signal<CheckedDraft | null>(null)

  /** How the check of the draft on screen ended; null once the draft changed since. */
  private readonly checkResult = computed<CheckResult | null>(() => {
    const checked = this.checked()
    const draft = this.draft()
    const current =
      checked !== null &&
      draft !== null &&
      checked.key === this.selectionKey() &&
      sameDraft(checked.draft, draft)
    return current ? checked.result : null
  })

  /** The recovery of the draft on screen, if another program got in before it. */
  private readonly recovery = signal<Recovery | null>(null)

  private readonly shownRecovery = computed(() => {
    const recovery = this.recovery()
    const draft = this.draft()
    const current =
      recovery?.phase === 'shown' &&
      recovery.key === this.selectionKey() &&
      (draft === null || sameDraft(recovery.draft, draft))
    return current ? recovery : null
  })

  /** The issues of the draft on screen: its recovery's, else its last check's. */
  private readonly issues = computed<readonly ProviderConfigFieldIssueDto[]>(() => {
    const recovery = this.shownRecovery()
    if (recovery !== null) return recovery.issues
    const result = this.checkResult()
    return result?.kind === 'issues' ? result.issues : []
  })

  private readonly checkFailure = computed<Failure | null>(() => {
    const result = this.checkResult()
    return result?.kind === 'failed' ? result.failure : null
  })

  /** A field's issue, as its validator reports it; a change elsewhere says what it is now. */
  private issueAt(at: ProviderConfigFieldRefDto): { kind: string; message: string } | undefined {
    const issue = this.issues().find(({ field }) => sameField(field, at))
    if (issue === undefined) return undefined
    const now = issue.problem === 'changed' ? this.currentValueOf(at) : null
    return {
      kind: 'providerConfigValue',
      message: now ? `${issue.message} Now: ${now}.` : issue.message,
    }
  }

  /** What a field holds in the snapshot on screen, for people; a secret only as set or not. */
  private currentValueOf(at: ProviderConfigFieldRefDto): string | null {
    const state = this.connectionState()
    if (state === null) return null
    if (at.kind === 'label') return state.label
    if (at.kind !== 'value') return null
    const field = state.fields.find(({ key }) => key === at.key)
    if (field === undefined) return null
    if (field.shape === 'list') return field.items.join(', ') || 'none'
    if (field.secret) return field.isSet ? 'set' : 'not set'
    return field.value ?? 'not set'
  }

  /**
   * What another program did to the connection as a whole since the draft's snapshot: removed
   * it, replaced it, or the snapshot is no longer known. The draft stays either way.
   */
  protected readonly connectionIssue = computed(
    () => this.issues().find(({ field }) => field.kind === 'connection') ?? null
  )

  /** Whether the draft on screen was refused for another program's change. */
  protected readonly conflicted = computed(() =>
    this.issues().some(({ problem }) => problem !== 'invalid')
  )

  /**
   * Another program removed the connection while it had a draft: the draft is kept, to discard
   * or to save into the source added again.
   */
  protected readonly removedDraft = computed(() => {
    const recovery = this.shownRecovery()
    return recovery !== null && this.connectionState() === null ? recovery : null
  })

  /**
   * Keeps the draft's edits over what another program saved: the draft is based on the snapshot
   * its recovery read, the one on screen, so its next save replaces only the fields it changes.
   */
  protected keepMyChanges(): void {
    const recovery = this.shownRecovery()
    if (recovery === null || recovery.revision !== this.snapshotRevision()) return
    this.recovery.set(null)
    this.draftBase.set(recovery.revision)
  }

  /** The draft of a removed source was discarded: the pane closes as after a removal. */
  protected afterDiscard(): void {
    this.recovery.set(null)
    this.removed.emit()
  }

  /**
   * The one way a conflict is handled, found by a blur check, a save or a removal: the snapshot
   * is read again, and the draft's own request is checked against it (see `Recovery`).
   */
  private beginRecovery(key: ConnectionKey): void {
    const draft = this.draft()
    const request = this.pendingRequest()
    if (draft === null || request === null) {
      this.config.reload()
      return
    }
    const recovery = this.recovery()
    if (recovery?.phase === 'reading' && recovery.key === key && sameDraft(recovery.draft, draft)) {
      return
    }
    this.recovery.set({ phase: 'reading', key, draft, request })
    this.config.reload()
  }

  /** Once the snapshot the recovery asked for is in: its request checked against it. */
  private async showRecovery(
    recovery: Extract<Recovery, { phase: 'reading' }>,
    revision: string
  ): Promise<void> {
    let issues: readonly ProviderConfigFieldIssueDto[]
    try {
      issues = await this.settings.checkProviderConfig(recovery.request)
    } catch (error) {
      if (this.recovery() !== recovery) return
      this.recovery.set(null)
      this.checked.set({
        key: recovery.key,
        draft: recovery.draft,
        result: { kind: 'failed', failure: failureOf(error, 'Checking the settings failed.') },
      })
      return
    }
    if (this.recovery() !== recovery) return
    const conflicting = issues.some(({ problem }) => problem !== 'invalid')
    this.recovery.set(
      conflicting
        ? { phase: 'shown', key: recovery.key, draft: recovery.draft, issues, revision }
        : null
    )
  }

  /**
   * Asks the backend what a save of the draft on screen would be refused for, as a field is left.
   * Its answer, or its failure, is the draft's it was sent for: dropped if that draft is no longer
   * the one on screen.
   */
  protected async checkDraft(): Promise<void> {
    const key = this.selectionKey()
    const draft = this.draft()
    const request = this.pendingRequest()
    if (!this.isDesktop || draft === null || request === null) return
    let result: CheckResult
    try {
      result = { kind: 'issues', issues: await this.settings.checkProviderConfig(request) }
    } catch (error) {
      result = { kind: 'failed', failure: failureOf(error, 'Checking the settings failed.') }
    }
    const now = this.draft()
    if (now === null || this.selectionKey() !== key || !sameDraft(now, draft)) return
    if (result.kind === 'issues' && result.issues.some(({ problem }) => problem !== 'invalid')) {
      this.beginRecovery(key)
      return
    }
    this.checked.set({ key, draft, result })
  }

  protected readonly draftForm = form(
    this.draftValues,
    path => {
      applyEach(path, item => {
        disabled(item, { when: () => this.savingConfig() })
        validate(item, ({ key }) => this.issueAt({ kind: 'value', key: key() }))
      })
    },
    {
      submission: {
        action: async () => {
          await this.saveConfig()
        },
      },
    }
  )

  /** Whether anything differs from what is saved. */
  protected readonly dirty = computed(() => this.pendingRequest() !== null)

  protected readonly canSave = computed(
    () =>
      this.isDesktop &&
      !this.loadingConfig() &&
      !this.savingConfig() &&
      this.pendingRequest() !== null &&
      !this.conflicted() &&
      this.labelForm().valid() &&
      this.draftForm().valid() &&
      this.listForm().valid()
  )

  constructor() {
    // A newer snapshot came in for a recovery on screen (the source was shown again): the
    // recovery follows it, so "Keep my changes" bases the draft on what is on screen.
    effect(() => {
      const recovery = this.shownRecovery()
      const revision = this.snapshotRevision()
      if (recovery === null || revision === null || this.loadingConfig()) return
      if (recovery.revision === revision) return
      untracked(() => {
        const request = this.pendingRequest()
        if (request !== null) {
          this.recovery.set({ phase: 'reading', key: recovery.key, draft: recovery.draft, request })
        }
      })
    })
    // A recovery's snapshot is in: the draft's request is checked against it.
    effect(() => {
      const recovery = this.recovery()
      if (recovery?.phase !== 'reading' || this.loadingConfig() || !this.config.hasValue()) return
      const revision = this.config.value().revision
      untracked(() => void this.showRecovery(recovery, revision))
    })
    // Keeps the edits of the loaded connection for when it is opened again; see `SourceDrafts`.
    effect(() => {
      const key = this.selectionKey()
      const draft = this.draft()
      if (draft === null) return
      if (!this.dirty()) {
        this.drafts.delete(key)
        return
      }
      this.drafts.set(key, draft)
    })
  }

  protected listItemError(key: string, index: number): string | null {
    const [error] = this.listForm[key][index]().errors()
    return error?.message ?? null
  }

  private listItemErrors(key: string): string[] {
    return (this.listDrafts()[key] ?? []).flatMap(
      (_, index) => this.listItemError(key, index) ?? []
    )
  }

  protected isListReset(field: ListField): boolean {
    return this.resetLists().has(field.key)
  }

  /** Drops every edit: the fields show what is saved again. */
  protected discard(): void {
    const state = this.connectionState()
    this.drafts.delete(this.selectionKey())
    if (state === null) return
    const saved = formOf(state)
    this.labelDraft.set(saved.label)
    this.draftValues.set(saved.values)
    this.listDrafts.set(saved.lists)
    this.resetLists.set(saved.resets)
  }

  protected addListItem(field: ListField): void {
    this.listDrafts.update(lists => ({ ...lists, [field.key]: [...(lists[field.key] ?? []), ''] }))
  }

  protected removeListItem(field: ListField, index: number): void {
    this.listDrafts.update(lists => ({
      ...lists,
      [field.key]: (lists[field.key] ?? []).filter((_, i) => i !== index),
    }))
  }

  /** Drops the configured list so the provider's default applies again. */
  protected resetList(field: ListField): void {
    this.resetLists.update(keys => new Set(keys).add(field.key))
  }

  protected undoListReset(field: ListField): void {
    this.resetLists.update(keys => {
      const next = new Set(keys)
      next.delete(field.key)
      return next
    })
  }

  protected fieldId(field: ProviderConfigFieldStateDto): string {
    return `${this.idPrefix()}-${field.key}`
  }

  protected labelFieldId(): string {
    return `${this.idPrefix()}-label`
  }

  private idPrefix(): string {
    const { source, name } = this.provider().info.connection
    return `provider-config-${source}-${name}`
  }

  protected async removeConnection(): Promise<void> {
    const state = this.connectionState()
    if (!state || this.removing()) return
    // The source asked about, whatever is shown once the question is answered, as the snapshot
    // on screen showed it.
    const {
      key,
      info: { label },
    } = this.provider()
    const basedOn = this.snapshotRevision() ?? undefined
    const confirmed = await confirmAction(this.dialog, {
      title: `Remove ${label}?`,
      message: 'Its settings are deleted from the config file. You can add it again later.',
      confirmLabel: 'Remove',
      tone: 'danger',
    })
    if (!confirmed) return
    // The draft the removal drops: one made while it is out stays.
    const revision = this.drafts.revisionOf(key)
    const removed = await this.operations.run(key, 'remove', 'Failed to remove the source.', () =>
      this.settings.removeConnection({ ...state.connection, basedOn })
    )
    if (!removed.ok) {
      // Another program changed it: what it is now shows before another attempt.
      const failure = this.operations.failure(key, 'remove')
      if (failure && refusedAs(failure, 'conflict') && this.selectionKey() === key) {
        this.beginRecovery(key)
      }
      return
    }
    this.toast.success(`${label} removed`)
    // Answered by a host that is gone, or one that shut down before it installed the removal:
    // the connection of that name on screen is not the one removed there, so its draft and its
    // section stay, and the snapshot is read again.
    const standing = this.data.receiptStanding(removed.value.installed)
    if (standing === 'otherHost' || standing === 'notInstalled') {
      this.config.reload()
      return
    }
    this.drafts.settled(key, revision)
    // Another source shown meanwhile stays.
    if (this.selectionKey() === key) this.removed.emit()
  }

  protected reload(): void {
    this.config.reload()
  }

  /** Marks a saved secret for removal; `null` in the draft means "clear on save". */
  protected clearSecret(field: SingleField): void {
    this.draftValues.update(values => ({ ...values, [field.key]: null }))
  }

  /** A saved secret is never shown, so its field says that one exists. */
  protected placeholderFor(field: SingleField): string {
    return this.isSecretFieldSet(field) ? 'Saved; type to replace' : ''
  }

  protected fieldError(key: string): string | null {
    if (!(key in this.draftValues())) {
      // A list another program changed as a whole, then its items.
      return this.issueAt({ kind: 'value', key })?.message ?? this.listItemErrors(key)[0] ?? null
    }

    const [error] = this.draftForm[key]().errors()
    if (!error) {
      return null
    }

    // A number input holding "-" or "1e" reports a parse error without a message.
    return error.kind === 'parse' ? 'Enter a whole number.' : (error.message ?? null)
  }

  /** The label's issue, such as another program's new label. */
  protected labelError(): string | null {
    const [error] = this.labelForm().errors()
    return error?.message ?? null
  }

  /** A saved secret that the draft has not cleared; plain fields show their value instead. */
  protected isSecretFieldSet(field: SingleField): boolean {
    return field.secret && field.isSet && this.draftValues()[field.key] !== null
  }

  protected inputTypeFor(field: SingleField): string {
    // Signal Forms parses a `null` model as a number in a text input, and a cleared secret is `null`.
    if (field.secret) {
      return 'password'
    }

    switch (field.valueType) {
      case 'int':
        return 'number'
      case 'email':
        return 'email'
      case 'url':
        return 'url'
      case 'password':
        return 'password'
      default:
        return 'text'
    }
  }

  protected async saveConfig(): Promise<void> {
    const request = this.pendingRequest()
    if (!request || !this.canSave()) {
      return
    }

    const draft = this.draft()
    if (draft === null) return
    const key = this.selectionKey()
    const label = request.label ?? this.provider().info.label
    // The draft sent: edits made after it (in a pane opened again) are not cleared by its answer.
    const revision = this.drafts.set(key, draft)
    const readsAtSend = this.snapshotReads
    const saved = await this.operations.run(key, 'save', 'Failed to save provider settings.', () =>
      this.settings.saveProviderConfig(request)
    )
    if (!saved.ok) {
      const failure = this.operations.failure(key, 'save')
      if (failure && refusedAs(failure, 'conflict') && this.selectionKey() === key) {
        this.beginRecovery(key)
      }
      return
    }
    // Before the new snapshot resets the fields, which would read the draft again.
    this.drafts.settled(key, revision)
    this.toast.success(`${label} saved`)
    // Another source shown meanwhile keeps its fields and edits; it reads the snapshot anew when
    // it is opened again.
    if (this.selectionKey() !== key) return
    // The answer's snapshot shows only if it is still the newest: of the host on screen, with no
    // later install of the sources on screen and no read of the file since the save was sent,
    // which may be newer than both. Otherwise the snapshot is read again.
    const newest =
      this.data.receiptStanding(saved.value.installed) === 'current' &&
      this.snapshotReads === readsAtSend
    if (newest) {
      this.config.set(saved.value.snapshot)
    } else {
      this.config.reload()
    }
  }
}

/** Why the snapshot could not be read, as the loader made it. */
function sameField(a: ProviderConfigFieldRefDto, b: ProviderConfigFieldRefDto): boolean {
  switch (a.kind) {
    case 'label':
      return b.kind === 'label'
    case 'value':
      return b.kind === 'value' && a.key === b.key
    case 'item':
      return b.kind === 'item' && a.key === b.key && a.index === b.index
    case 'connection':
      return b.kind === 'connection'
  }
}

function loadFailureOf(error: Error | undefined): Failure | null {
  if (error === undefined) return null
  return error instanceof FailureError ? error.failure : failureOf(error)
}
