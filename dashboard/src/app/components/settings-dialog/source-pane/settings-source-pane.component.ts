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
  ProviderConfigFieldStateDto,
  ProviderConfigSaveRequestDto,
  ProviderProblem,
  ProviderStatus,
} from '../../../generated/activity'
import { connectionKey } from '../../../providers/connection'
import { supportedOsLabel } from '../../../providers/source-catalog'
import { ActivityService } from '../../../services/activity.service'
import { extractBackendErrorMessage } from '../../../services/backend/error-message'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
import { ToastService } from '../../../services/toast.service'
import { confirmAction } from '../../confirm-dialog/confirm-dialog.component'
import { validateProviderConfigValue } from './provider-config-validation'
import type { SettingsProviderRow } from '../settings-dialog.types'
import { type SourceDraft, SourceDrafts } from './source-drafts.service'

interface ProviderHealthVm {
  readonly icon: string
  readonly title: string
  readonly reason: string
  /** The startup check has not finished; `checkedAt` is when it started. */
  readonly checking: boolean
}

/** Field values by key. A secret is `null` once the user chose to clear it, `''` while untouched. */
type DraftValues = Record<string, string | null>
/** Items of `list` fields by key; blank rows are dropped on save. */
type ListDrafts = Record<string, string[]>

type SingleField = Extract<ProviderConfigFieldStateDto, { shape: 'single' }>
type ListField = Extract<ProviderConfigFieldStateDto, { shape: 'list' }>

function isSingleField(field: ProviderConfigFieldStateDto): field is SingleField {
  return field.shape === 'single'
}

function isListField(field: ProviderConfigFieldStateDto): field is ListField {
  return field.shape === 'list'
}

/** Trimmed, without blank rows: what the backend would store. */
function cleanItems(items: readonly string[]): string[] {
  return items.map(item => item.trim()).filter(item => item !== '')
}

function sameItems(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((item, index) => item === b[index])
}

@Component({
  selector: 'app-settings-source-pane',
  imports: [
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
  protected readonly savingConfig = signal(false)

  private readonly selectionKey = computed(() => this.provider().key)

  /** The snapshot holds every connection; it reloads whenever another one is selected. */
  private readonly config = resource({
    params: () => (this.isDesktop ? this.selectionKey() : undefined),
    loader: () =>
      this.activity.loadProviderConfig().catch((error: unknown) => {
        throw new Error(extractBackendErrorMessage(error, 'Failed to load provider settings.'))
      }),
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

  /** The label the user is typing. */
  protected readonly labelDraft = linkedSignal(
    () => this.savedDraft()?.label ?? this.connectionState()?.label ?? ''
  )
  protected readonly labelForm = form(this.labelDraft, path => {
    disabled(path, { when: () => this.savingConfig() })
  })
  protected readonly removing = signal(false)

  private readonly fieldByKey = computed(
    () => new Map(this.configFields().map(field => [field.key, field]))
  )

  private readonly initialValues = computed<DraftValues>(() =>
    Object.fromEntries(
      this.configFields()
        .filter(isSingleField)
        .map(field => [field.key, this.initialValueFor(field)])
    )
  )
  private readonly draftValues = linkedSignal<DraftValues>(() => ({
    ...this.initialValues(),
    ...this.savedDraft()?.values,
  }))

  /** The configured items, or the default the provider uses while the key is unset. */
  private readonly initialLists = computed<ListDrafts>(() =>
    Object.fromEntries(
      this.configFields()
        .filter(isListField)
        .map(field => [field.key, [...field.items]])
    )
  )
  protected readonly listDrafts = linkedSignal<ListDrafts>(() => {
    const drafted = this.savedDraft()?.lists ?? {}
    return Object.fromEntries(
      Object.entries(this.initialLists()).map(([key, items]) => [key, [...(drafted[key] ?? items)]])
    )
  })
  /** List keys the user reset, so their default applies again after saving. */
  private readonly resetLists = linkedSignal<ListDrafts, ReadonlySet<string>>({
    source: this.initialLists,
    computation: () => new Set(this.savedDraft()?.resets),
  })

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

  private readonly saveError = linkedSignal<string, string | null>({
    source: this.selectionKey,
    computation: () => null,
  })
  protected readonly configError = computed(
    () => this.saveError() ?? this.config.error()?.message ?? null
  )

  protected readonly health = computed<ProviderHealthVm>(() => {
    const { status, reason, problem } = this.provider().info
    const checking = status === 'checking'
    return {
      icon: problem === 'timedOut' ? 'timer_off' : this.statusIcon(status),
      title: this.statusTitle(status, problem),
      reason:
        reason ??
        (checking
          ? 'The startup check is still running.'
          : 'No issues were reported during the latest provider inspection.'),
      checking,
    }
  })

  /** A source whose check or collection failed can be checked again; one on another OS cannot. */
  protected readonly canRecheck = computed(() => {
    const { status, problem } = this.provider().info
    return (status === 'unavailable' || status === 'degraded') && problem !== 'unsupported'
  })
  /** From the press until the collection after the new check has ended. */
  protected readonly rechecking = signal(false)

  protected async recheck(): Promise<void> {
    this.rechecking.set(true)
    try {
      await this.activity.recheckConnection(this.provider().info.connection)
    } finally {
      this.rechecking.set(false)
    }
  }

  protected readonly checkedAtLabel = computed(() => {
    const checkedAt = new Date(this.provider().info.checkedAt)
    if (Number.isNaN(checkedAt.getTime())) return 'at startup'
    return checkedAt.toLocaleString('en-GB', {
      day: 'numeric',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
    })
  })

  protected readonly pendingRequest = computed<ProviderConfigSaveRequestDto | null>(() => {
    const values: ProviderConfigSaveRequestDto['values'] = []
    const draft = this.draftValues()
    const initial = this.initialValues()

    for (const field of this.configFields()) {
      if (isListField(field)) {
        const change = this.pendingListValue(field)
        if (change !== undefined) values.push({ key: field.key, value: change })
        continue
      }

      const value = draft[field.key] ?? null

      if (field.secret) {
        if (value === null) {
          values.push({ key: field.key, value: null })
        } else if (value.trim()) {
          values.push({ key: field.key, value: value.trim() })
        }
        continue
      }

      const current = (value ?? '').trim()
      if (current !== (initial[field.key] ?? '')) {
        values.push({
          key: field.key,
          value: current === '' ? null : current,
        })
      }
    }

    const state = this.connectionState()
    if (!state) {
      return null
    }
    const label = this.labelDraft().trim()
    const labelChanged = label !== '' && label !== state.label
    if (values.length === 0 && !labelChanged) {
      return null
    }

    return {
      connection: state.connection,
      ...(labelChanged ? { label } : {}),
      values,
    }
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

  protected readonly supportedOsLabel = supportedOsLabel

  constructor() {
    // Keeps the edits of the loaded connection for when it is opened again; see `SourceDrafts`.
    effect(() => {
      const key = this.selectionKey()
      if (this.connectionState() === null) return
      if (!this.dirty()) {
        this.drafts.delete(key)
        return
      }
      this.drafts.set(key, {
        label: this.labelDraft(),
        values: this.draftValues(),
        lists: this.listDrafts(),
        resets: [...this.resetLists()],
      })
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
    this.drafts.delete(this.selectionKey())
    this.labelDraft.set(this.connectionState()?.label ?? '')
    this.draftValues.set({ ...this.initialValues() })
    this.listDrafts.set(
      Object.fromEntries(
        Object.entries(this.initialLists()).map(([key, items]) => [key, [...items]])
      )
    )
    this.resetLists.set(new Set())
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
    const confirmed = await confirmAction(this.dialog, {
      title: `Remove ${this.provider().label}?`,
      message: 'Its settings are deleted from the config file. You can add it again later.',
      confirmLabel: 'Remove',
      tone: 'danger',
    })
    if (!confirmed) return
    const label = this.provider().label
    this.removing.set(true)
    this.saveError.set(null)
    try {
      await this.activity.removeConnection(state.connection)
      this.drafts.delete(this.selectionKey())
      this.toast.success(`${label} removed`)
      this.removed.emit()
    } catch (error) {
      this.saveError.set(extractBackendErrorMessage(error, 'Failed to remove the source.'))
    } finally {
      this.removing.set(false)
    }
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

    this.savingConfig.set(true)
    this.saveError.set(null)

    try {
      const response = await this.activity.saveProviderConfig(request)
      // Before the new snapshot resets the fields, which would read the draft again.
      this.drafts.delete(this.selectionKey())
      this.config.set(response.snapshot)
      this.toast.success(`${request.label ?? this.provider().label} saved`)
    } catch (error) {
      this.saveError.set(extractBackendErrorMessage(error, 'Failed to save provider settings.'))
    } finally {
      this.savingConfig.set(false)
    }
  }

  /**
   * `null` resets a configured list to its default, an array replaces it (empty means none), and
   * `undefined` means unchanged.
   */
  private pendingListValue(field: ListField): string[] | null | undefined {
    if (this.resetLists().has(field.key)) {
      return field.isSet ? null : undefined
    }

    const items = cleanItems(this.listDrafts()[field.key] ?? [])
    return sameItems(items, this.initialLists()[field.key] ?? []) ? undefined : items
  }

  private initialValueFor(field: SingleField): string {
    const value = field.secret ? '' : (field.value ?? '')
    if (field.valueType !== 'bool') {
      return value
    }

    const normalized = value.trim().toLowerCase()
    return normalized === 'true' || normalized === 'false' ? normalized : value
  }

  private statusIcon(status: ProviderStatus): string {
    switch (status) {
      case 'checking':
        return 'hourglass_empty'
      case 'available':
        return 'check_circle'
      case 'degraded':
        return 'warning_amber'
      case 'unavailable':
        return 'block'
    }
  }

  private statusTitle(status: ProviderStatus, problem: ProviderProblem | undefined): string {
    switch (status) {
      case 'checking':
        return 'Checking'
      case 'available':
        return 'Ready to collect'
      case 'degraded':
        return 'Running with issues'
      case 'unavailable':
        switch (problem) {
          case 'timedOut':
            return 'Startup check timed out'
          case 'unsupported':
            return 'Not supported on this system'
          case 'notConfigured':
            return 'Not configured'
          case 'failed':
          case undefined:
            return 'Not available'
        }
    }
  }
}
