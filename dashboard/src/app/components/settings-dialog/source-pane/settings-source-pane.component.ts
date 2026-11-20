import {
  Component,
  computed,
  effect,
  inject,
  input,
  linkedSignal,
  output,
  resource,
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
  ProviderConfigFieldStateDto,
  ProviderConfigSaveRequestDto,
} from '../../../generated/activity'
import { connectionKey } from '../../../providers/connection'
import { ActivityService } from '../../../services/activity.service'
import { type Failure, FailureError, failureOf } from '../../../services/failure'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
import { ToastService } from '../../../services/toast.service'
import { confirmAction } from '../../confirm-dialog/confirm-dialog.component'
import { validateProviderConfigValue } from './provider-config-validation'
import type { SettingsProviderRow } from '../settings-dialog.types'
import { SourceDrafts } from './source-drafts.service'
import { SourceHealthComponent } from './source-health/source-health.component'
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
  isListField,
  isSingleField,
  saveRequestOf,
} from './source-draft-request'

@Component({
  selector: 'app-settings-source-pane',
  imports: [
    SourceHealthComponent,
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
  private readonly activity = inject(ActivityService)
  private readonly runtime = inject(RuntimeEnvironmentService)
  private readonly dialog = inject(MatDialog)
  private readonly toast = inject(ToastService)
  private readonly drafts = inject(SourceDrafts)

  readonly provider = input.required<SettingsProviderRow>()
  /** Emitted once the connection was removed. */
  readonly removed = output<void>()

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
        const host = untracked(this.activity.hostEpoch)
        try {
          const snapshot = await this.activity.loadProviderConfig()
          if (untracked(this.activity.hostEpoch) === host) {
            this.snapshotReads++
            return snapshot
          }
        } catch (error) {
          if (untracked(this.activity.hostEpoch) === host) {
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
  })
  protected readonly removing = computed(() =>
    this.operations.isRunning(this.selectionKey(), 'remove')
  )

  private readonly fieldByKey = computed(
    () => new Map(this.configFields().map(field => [field.key, field]))
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

  /** Every item is checked against the field's value type, like a single value. */
  protected readonly listForm = form(this.listDrafts, path => {
    applyEach(path, list => {
      applyEach(list, item => {
        disabled(item, { when: () => this.savingConfig() })
        // `pathKeys` is `[fieldKey, index]`.
        validate(item, ({ value, pathKeys }) => {
          const field = this.fieldByKey().get(pathKeys()[0] ?? '')
          const text = value().trim()
          if (!field || !isListField(field) || text === '') return undefined
          const message = validateProviderConfigValue(text, field.itemType)
          return message ? { kind: 'providerConfigValue', message } : undefined
        })
      })
    })
  })

  protected readonly configError = computed(() => {
    const key = this.selectionKey()
    const failure =
      this.operations.failure(key, 'save') ??
      this.operations.failure(key, 'remove') ??
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
      this.activity.recheckConnection(info.connection)
    )
  }

  /** What the user changed, against the settings on screen; null before they are loaded. */
  private readonly draft = computed<SourceDraft | null>(() => {
    const state = this.connectionState()
    return (
      state &&
      draftOf(state, {
        label: this.labelDraft(),
        values: this.draftValues(),
        lists: this.listDrafts(),
        resets: this.resetLists(),
      })
    )
  })

  /** The edits as one save request for this connection, or null when nothing changed. */
  protected readonly pendingRequest = computed<ProviderConfigSaveRequestDto | null>(() => {
    const state = this.connectionState()
    const draft = this.draft()
    return state && draft && saveRequestOf(state, draft)
  })

  /** Only changed values are checked, so a stored value that no longer validates never blocks saving. */
  private readonly pendingValues = computed(
    () =>
      new Map(
        this.pendingRequest()?.values.flatMap(({ key, value }) =>
          Array.isArray(value) ? [] : [[key, value] as const]
        )
      )
  )

  protected readonly draftForm = form(
    this.draftValues,
    path => {
      applyEach(path, item => {
        disabled(item, { when: () => this.savingConfig() })
        validate(item, ({ key }) => {
          const field = this.fieldByKey().get(key())
          const value = this.pendingValues().get(key())
          if (!field || !isSingleField(field) || value === null || value === undefined) {
            return undefined
          }

          const message = validateProviderConfigValue(value, field.valueType)
          return message ? { kind: 'providerConfigValue', message } : undefined
        })
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
      this.draftForm().valid() &&
      this.listForm().valid()
  )

  constructor() {
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
    // The source asked about, whatever is shown once the question is answered.
    const { key, label } = this.provider()
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
      this.activity.removeConnection(state.connection)
    )
    if (!removed.ok) return
    this.toast.success(`${label} removed`)
    // Answered by a host that is gone, or one that shut down before it installed the removal:
    // the connection of that name on screen is not the one removed there, so its draft and its
    // section stay, and the snapshot is read again.
    const standing = this.activity.receiptStanding(removed.value.installed)
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
      return this.listItemErrors(key)[0] ?? null
    }

    const [error] = this.draftForm[key]().errors()
    if (!error) {
      return null
    }

    // A number input holding "-" or "1e" reports a parse error without a message.
    return error.kind === 'parse' ? 'Enter a whole number.' : (error.message ?? null)
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
    const label = request.label ?? this.provider().label
    // The draft sent: edits made after it (in a pane opened again) are not cleared by its answer.
    const revision = this.drafts.set(key, draft)
    const readsAtSend = this.snapshotReads
    const saved = await this.operations.run(key, 'save', 'Failed to save provider settings.', () =>
      this.activity.saveProviderConfig(request)
    )
    if (!saved.ok) return
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
      this.activity.receiptStanding(saved.value.installed) === 'current' &&
      this.snapshotReads === readsAtSend
    if (newest) {
      this.config.set(saved.value.snapshot)
    } else {
      this.config.reload()
    }
  }
}

/** Why the snapshot could not be read, as the loader made it. */
function loadFailureOf(error: Error | undefined): Failure | null {
  if (error === undefined) return null
  return error instanceof FailureError ? error.failure : failureOf(error)
}
