import { Component, computed, inject, input, linkedSignal, resource, signal } from '@angular/core'
import { applyEach, disabled, form, FormField, FormRoot, validate } from '@angular/forms/signals'
import { MatFormFieldModule } from '@angular/material/form-field'
import { MatInputModule } from '@angular/material/input'
import { MatSelectModule } from '@angular/material/select'
import type {
  ProviderConfigFieldStateDto,
  ProviderConfigSaveRequestDto,
  ProviderStatus,
  SupportedOs,
} from '../../../generated/activity'
import { ActivityService } from '../../../services/activity.service'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
import { StatusPillComponent } from '../../ui-status/status-pill.component'
import { validateProviderConfigValue } from './provider-config-validation'
import type { SettingsProviderRow } from '../settings-dialog.types'

interface ProviderHealthVm {
  readonly icon: string
  readonly title: string
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

const TIMEOUT_REASON = /timed? ?out/i

function errorMessage(error: unknown, fallback: string): string {
  if (error instanceof Error) {
    return error.message
  }

  // Tauri commands reject with the backend's error string.
  if (typeof error === 'string' && error.trim()) {
    return error
  }

  return fallback
}

@Component({
  selector: 'app-settings-source-pane',
  imports: [
    FormField,
    FormRoot,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    StatusPillComponent,
  ],
  templateUrl: './settings-source-pane.component.html',
  styleUrl: './settings-source-pane.component.scss',
})
export class SettingsSourcePaneComponent {
  private readonly activity = inject(ActivityService)
  private readonly runtime = inject(RuntimeEnvironmentService)

  readonly provider = input.required<SettingsProviderRow>()

  protected readonly isDesktop = this.runtime.isTauri()
  protected readonly savingConfig = signal(false)

  private readonly selectionKey = computed(() => this.provider().info.id)

  /** The snapshot holds every provider; it reloads whenever another source is selected. */
  private readonly config = resource({
    params: () => (this.isDesktop ? this.selectionKey() : undefined),
    loader: () =>
      this.activity.loadProviderConfig().catch((error: unknown) => {
        throw new Error(errorMessage(error, 'Failed to load provider settings.'))
      }),
  })

  protected readonly loadingConfig = this.config.isLoading

  protected readonly configFields = computed(() => {
    // `value()` throws while the resource is in its error state.
    if (!this.config.hasValue()) {
      return []
    }

    const source = this.selectionKey()
    return this.config.value().providers.find(provider => provider.source === source)?.fields ?? []
  })

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
  private readonly draftValues = linkedSignal<DraftValues>(() => ({ ...this.initialValues() }))

  /** The configured items, or the default the provider uses while the key is unset. */
  private readonly initialLists = computed<ListDrafts>(() =>
    Object.fromEntries(
      this.configFields()
        .filter(isListField)
        .map(field => [field.key, [...field.items]])
    )
  )
  protected readonly listDrafts = linkedSignal<ListDrafts>(() =>
    Object.fromEntries(Object.entries(this.initialLists()).map(([key, items]) => [key, [...items]]))
  )
  /** List keys the user reset, so their default applies again after saving. */
  private readonly resetLists = linkedSignal<ListDrafts, ReadonlySet<string>>({
    source: this.initialLists,
    computation: () => new Set(),
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
  /** Hidden again by any edit, and by switching source. */
  protected readonly configMessage = linkedSignal<unknown, string | null>({
    source: () => [this.draftValues(), this.listDrafts(), this.resetLists()],
    computation: () => null,
  })
  protected readonly configError = computed(
    () => this.saveError() ?? this.config.error()?.message ?? null
  )

  protected readonly health = computed<ProviderHealthVm>(() => {
    const { status, reason } = this.provider().info
    const timedOut = reason !== undefined && TIMEOUT_REASON.test(reason)
    return {
      icon: timedOut ? 'timer_off' : this.statusIcon(status),
      title: this.statusTitle(status, timedOut),
    }
  })

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

    if (values.length === 0) {
      return null
    }

    return {
      source: this.selectionKey(),
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

  protected readonly canSave = computed(
    () =>
      this.isDesktop &&
      !this.loadingConfig() &&
      !this.savingConfig() &&
      this.pendingRequest() !== null &&
      this.draftForm().valid() &&
      this.listForm().valid()
  )

  protected supportedOsLabel(os: SupportedOs): string {
    switch (os) {
      case 'linux':
        return 'Linux'
      case 'macos':
        return 'macOS'
      case 'windows':
        return 'Windows'
    }
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
    return `provider-config-${this.provider().info.id}-${field.key}`
  }

  protected reload(): void {
    this.config.reload()
  }

  protected clearField(field: SingleField): void {
    this.draftValues.update(values => ({ ...values, [field.key]: field.secret ? null : '' }))
  }

  protected canClearField(field: SingleField): boolean {
    if (field.secret) {
      return (this.draftValues()[field.key] ?? '').trim().length > 0 || this.isSecretFieldSet(field)
    }

    return (this.draftValues()[field.key] ?? '').trim().length > 0
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

  protected isSecretFieldSet(field: SingleField): boolean {
    return field.isSet && this.draftValues()[field.key] !== null
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
    this.configMessage.set(null)

    try {
      const response = await this.activity.saveProviderConfig(request)
      this.config.set(response.snapshot)
      this.configMessage.set('Settings saved.')
    } catch (error) {
      this.saveError.set(errorMessage(error, 'Failed to save provider settings.'))
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
      case 'available':
        return 'check_circle'
      case 'degraded':
        return 'warning_amber'
      case 'unavailable':
        return 'block'
    }
  }

  private statusTitle(status: ProviderStatus, timedOut: boolean): string {
    switch (status) {
      case 'available':
        return 'Ready to collect'
      case 'degraded':
        return timedOut ? 'Responding slowly' : 'Running with issues'
      case 'unavailable':
        return timedOut ? 'Startup check timed out' : 'Not available'
    }
  }
}
