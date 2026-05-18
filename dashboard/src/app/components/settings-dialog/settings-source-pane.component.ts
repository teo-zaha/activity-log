import {
  Component,
  computed,
  effect,
  inject,
  input,
  signal,
  ChangeDetectionStrategy,
} from '@angular/core'
import { FormsModule } from '@angular/forms'
import { MatButtonModule } from '@angular/material/button'
import { MatCardModule } from '@angular/material/card'
import { MatChipsModule } from '@angular/material/chips'
import { MatDividerModule } from '@angular/material/divider'
import { MatFormFieldModule } from '@angular/material/form-field'
import { MatInputModule } from '@angular/material/input'
import { MatSelectModule } from '@angular/material/select'
import type {
  ProviderConfigFieldStateDto,
  ProviderConfigSaveRequestDto,
  ProviderConfigSnapshotDto,
  Source,
  SupportedOs,
} from '../../generated/activity'
import { ActivityService } from '../../services/activity.service'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'
import { StatusPillComponent } from '../ui-status/status-pill.component'
import { validateProviderConfigValue } from './provider-config-validation'
import type { SettingsProviderRow } from './settings-dialog.types'

@Component({
  selector: 'app-settings-source-pane',
  imports: [
    FormsModule,
    MatButtonModule,
    MatCardModule,
    MatChipsModule,
    MatDividerModule,
    MatFormFieldModule,
    MatInputModule,
    MatSelectModule,
    StatusPillComponent,
  ],
  templateUrl: './settings-source-pane.component.html',
  styleUrl: './settings-source-pane.component.scss',
  changeDetection: ChangeDetectionStrategy.OnPush,
})
export class SettingsSourcePaneComponent {
  private readonly activity = inject(ActivityService)
  private readonly runtime = inject(RuntimeEnvironmentService)

  readonly provider = input.required<SettingsProviderRow>()

  protected readonly isDesktop = this.runtime.isTauri()
  protected readonly loadingConfig = signal(false)
  protected readonly savingConfig = signal(false)
  protected readonly configError = signal<string | null>(null)
  protected readonly configMessage = signal<string | null>(null)

  private readonly snapshot = signal<ProviderConfigSnapshotDto | null>(null)
  private readonly draftValues = signal<Record<string, string>>({})
  private readonly initialValues = signal<Record<string, string>>({})
  private readonly clearedSecretKeys = signal<Set<string>>(new Set())

  protected readonly configProvider = computed(() => {
    const snapshot = this.snapshot()
    if (!snapshot) {
      return null
    }

    return snapshot.providers.find(provider => provider.source === this.provider().info.id)
  })

  protected readonly configFields = computed(() => this.configProvider()?.fields ?? [])

  protected readonly pendingRequest = computed<ProviderConfigSaveRequestDto | null>(() => {
    const fields = this.configFields()
    const values: ProviderConfigSaveRequestDto['values'] = []
    const draft = this.draftValues()
    const initial = this.initialValues()
    const clearedSecrets = this.clearedSecretKeys()

    for (const field of fields) {
      const current = (draft[field.key] ?? '').trim()

      if (field.secret) {
        if (clearedSecrets.has(field.key)) {
          values.push({ key: field.key, value: null })
          continue
        }

        if (current) {
          values.push({ key: field.key, value: current })
        }
        continue
      }

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
      source: this.provider().info.id,
      values,
    }
  })

  protected readonly validationErrors = computed(() => {
    const errors = new Map<string, string>()
    const request = this.pendingRequest()
    const fields = this.configFields()

    if (!request) {
      return errors
    }

    const fieldByKey = new Map(fields.map(field => [field.key, field]))
    for (const value of request.values) {
      if (value.value === null) {
        continue
      }

      const field = fieldByKey.get(value.key)
      if (!field) {
        continue
      }

      const error = validateProviderConfigValue(value.value, field.valueType)
      if (error) {
        errors.set(value.key, error)
      }
    }

    return errors
  })

  protected readonly canSave = computed(
    () =>
      this.isDesktop &&
      !this.loadingConfig() &&
      !this.savingConfig() &&
      this.pendingRequest() !== null &&
      this.validationErrors().size === 0
  )

  constructor() {
    effect(() => {
      const source = this.provider().info.id
      this.configMessage.set(null)
      this.configError.set(null)

      if (!this.isDesktop) {
        return
      }

      void this.loadProviderConfig(source)
    })
  }

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

  protected fieldValue(field: ProviderConfigFieldStateDto): string {
    return this.draftValues()[field.key] ?? ''
  }

  protected setFieldValue(field: ProviderConfigFieldStateDto, rawValue: string): void {
    this.draftValues.update(values => ({
      ...values,
      [field.key]: this.normalizeDraftValue(rawValue, field.valueType),
    }))

    this.clearedSecretKeys.update(previous => {
      if (!previous.has(field.key)) {
        return previous
      }

      const next = new Set(previous)
      next.delete(field.key)
      return next
    })

    this.configMessage.set(null)
  }

  protected clearField(field: ProviderConfigFieldStateDto): void {
    this.draftValues.update(values => ({
      ...values,
      [field.key]: '',
    }))

    if (field.secret) {
      this.clearedSecretKeys.update(previous => {
        const next = new Set(previous)
        next.add(field.key)
        return next
      })
    }

    this.configMessage.set(null)
  }

  protected canClearField(field: ProviderConfigFieldStateDto): boolean {
    const current = this.fieldValue(field).trim()
    if (field.secret) {
      return current.length > 0 || this.isSecretFieldSet(field)
    }

    return current.length > 0
  }

  protected fieldError(key: string): string | null {
    return this.validationErrors().get(key) ?? null
  }

  protected isSecretFieldSet(field: ProviderConfigFieldStateDto): boolean {
    if (this.clearedSecretKeys().has(field.key)) {
      return false
    }

    return field.isSet
  }

  protected inputTypeFor(field: ProviderConfigFieldStateDto): string {
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
    this.configError.set(null)
    this.configMessage.set(null)

    try {
      const response = await this.activity.saveProviderConfig(request)
      this.applySnapshot(response.snapshot)
      this.configMessage.set('Settings saved.')
    } catch (error) {
      this.configError.set(this.errorMessage(error))
    } finally {
      this.savingConfig.set(false)
    }
  }

  private async loadProviderConfig(_source: Source): Promise<void> {
    this.loadingConfig.set(true)

    try {
      const snapshot = await this.activity.loadProviderConfig()
      this.applySnapshot(snapshot)
    } catch (error) {
      this.snapshot.set(null)
      this.initialValues.set({})
      this.draftValues.set({})
      this.clearedSecretKeys.set(new Set())
      this.configError.set(this.errorMessage(error))
    } finally {
      this.loadingConfig.set(false)
    }
  }

  private applySnapshot(snapshot: ProviderConfigSnapshotDto): void {
    this.snapshot.set(snapshot)

    const provider = snapshot.providers.find(item => item.source === this.provider().info.id)
    const initialValues: Record<string, string> = {}

    for (const field of provider?.fields ?? []) {
      initialValues[field.key] = this.initialValueFor(field)
    }

    this.initialValues.set(initialValues)
    this.draftValues.set({ ...initialValues })
    this.clearedSecretKeys.set(new Set())
  }

  private initialValueFor(field: ProviderConfigFieldStateDto): string {
    if (field.secret) {
      return ''
    }

    return this.normalizeDraftValue(field.value ?? '', field.valueType)
  }

  private normalizeDraftValue(
    value: string,
    valueType: ProviderConfigFieldStateDto['valueType']
  ): string {
    if (valueType !== 'bool') {
      return value
    }

    const normalized = value.trim().toLowerCase()
    if (normalized === 'true') {
      return 'true'
    }

    if (normalized === 'false') {
      return 'false'
    }

    return value
  }

  private errorMessage(error: unknown): string {
    if (error instanceof Error) {
      return error.message
    }

    return 'Failed to load provider settings.'
  }
}
