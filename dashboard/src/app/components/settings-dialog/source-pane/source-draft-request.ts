import type {
  ProviderConfigConnectionStateDto,
  ProviderConfigFieldStateDto,
  ProviderConfigSaveRequestDto,
} from '../../../generated/activity'

// A connection's edits, kept as what the user asked for (`SourceDraft`), the fields that show
// them (`DraftForm`) and the save request they make. A field the user did not change is not in
// the draft, so it is never sent: a value changed elsewhere meanwhile stays as it is.

/** Field values by key, as the inputs hold them; a secret the user cleared is `null`. */
export type DraftValues = Record<string, string | null>
/** Items of `list` fields by key; blank rows are dropped on save. */
export type ListDrafts = Record<string, string[]>

export type SingleField = Extract<ProviderConfigFieldStateDto, { shape: 'single' }>
export type ListField = Extract<ProviderConfigFieldStateDto, { shape: 'list' }>

/** What a draft asks of one single-value field. */
export type ValueEdit =
  | { readonly kind: 'untouched' }
  | { readonly kind: 'clear' }
  | { readonly kind: 'replace'; readonly value: string }

/** What a draft asks of one list field. */
export type ListEdit =
  | { readonly kind: 'untouched' }
  /** Back to the provider's default. */
  | { readonly kind: 'reset' }
  /** Empty means none. */
  | { readonly kind: 'replace'; readonly items: readonly string[] }

/** A connection's unsaved edits: only what the user changed, and what they changed it from. */
export interface SourceDraft {
  /**
   * The snapshot `revision` the edits were made against: the save sends it as `basedOn`, so a
   * field another program changed meanwhile is a conflict, not overwritten.
   */
  readonly basedOn: string
  /** The label typed, or null for the saved one. */
  readonly label: string | null
  readonly values: Readonly<Record<string, Exclude<ValueEdit, { kind: 'untouched' }>>>
  readonly lists: Readonly<Record<string, Exclude<ListEdit, { kind: 'untouched' }>>>
}

/** What the pane's inputs hold. */
export interface DraftForm {
  readonly label: string
  readonly values: DraftValues
  readonly lists: ListDrafts
  /** List keys reset to their default. */
  readonly resets: ReadonlySet<string>
}

export function isSingleField(field: ProviderConfigFieldStateDto): field is SingleField {
  return field.shape === 'single'
}

export function isListField(field: ProviderConfigFieldStateDto): field is ListField {
  return field.shape === 'list'
}

/** What each single field shows before an edit: plain values as saved, secrets empty. */
export function initialValues(fields: readonly ProviderConfigFieldStateDto[]): DraftValues {
  return Object.fromEntries(
    fields.filter(isSingleField).map(field => [field.key, initialValueFor(field)])
  )
}

/** The configured items, or the default the provider uses while the key is unset. */
export function initialLists(fields: readonly ProviderConfigFieldStateDto[]): ListDrafts {
  return Object.fromEntries(fields.filter(isListField).map(field => [field.key, [...field.items]]))
}

const UNTOUCHED = { kind: 'untouched' } as const

/**
 * The edit an input's value makes. A secret is cleared only by an explicit `null` and replaced
 * by text; a plain value is compared with what is saved, and emptied means cleared.
 */
export function valueEditOf(field: SingleField, drafted: string | null | undefined): ValueEdit {
  if (drafted === undefined) return UNTOUCHED
  if (field.secret) {
    if (drafted === null) return { kind: 'clear' }
    const value = drafted.trim()
    return value === '' ? UNTOUCHED : { kind: 'replace', value }
  }
  // Both trimmed, as saving would store the input: a saved value with outer spaces, shown and
  // not edited, is no edit.
  const value = (drafted ?? '').trim()
  if (value === initialValueFor(field).trim()) return UNTOUCHED
  return value === '' ? { kind: 'clear' } : { kind: 'replace', value }
}

/** The edit a list's rows make, or its reset. */
export function listEditOf(
  field: ListField,
  drafted: readonly string[] | undefined,
  reset: boolean
): ListEdit {
  if (reset) return field.isSet ? { kind: 'reset' } : UNTOUCHED
  if (drafted === undefined) return UNTOUCHED
  const items = cleanItems(drafted)
  return sameItems(items, field.items) ? UNTOUCHED : { kind: 'replace', items }
}

/** What the user changed in `form` against `state`'s saved settings. */
export function draftOf(
  state: ProviderConfigConnectionStateDto,
  form: DraftForm,
  basedOn: string
): SourceDraft {
  const values: Record<string, Exclude<ValueEdit, { kind: 'untouched' }>> = {}
  const lists: Record<string, Exclude<ListEdit, { kind: 'untouched' }>> = {}
  for (const field of state.fields) {
    if (isListField(field)) {
      const edit = listEditOf(field, form.lists[field.key], form.resets.has(field.key))
      if (edit.kind !== 'untouched') lists[field.key] = edit
    } else {
      const edit = valueEditOf(field, form.values[field.key])
      if (edit.kind !== 'untouched') values[field.key] = edit
    }
  }
  const label = form.label.trim()
  return { basedOn, label: label !== '' && label !== state.label ? label : null, values, lists }
}

/** Whether two drafts ask for the same: the same label and the same edits. */
export function sameDraft(a: SourceDraft, b: SourceDraft): boolean {
  return (
    a.basedOn === b.basedOn &&
    a.label === b.label &&
    sameEdits(a.values, b.values, (x, y) =>
      x.kind === 'clear' ? y.kind === 'clear' : y.kind === 'replace' && x.value === y.value
    ) &&
    sameEdits(a.lists, b.lists, (x, y) =>
      x.kind === 'reset' ? y.kind === 'reset' : y.kind === 'replace' && sameItems(x.items, y.items)
    )
  )
}

function sameEdits<Edit>(
  a: Readonly<Record<string, Edit>>,
  b: Readonly<Record<string, Edit>>,
  same: (x: Edit, y: Edit) => boolean
): boolean {
  const keys = Object.keys(a)
  return (
    keys.length === Object.keys(b).length &&
    keys.every(key => {
      const x = a[key]
      const y = b[key]
      return x !== undefined && y !== undefined && same(x, y)
    })
  )
}

/** The inputs for `state`'s saved settings with `draft`'s edits over them. */
export function formOf(state: ProviderConfigConnectionStateDto, draft?: SourceDraft): DraftForm {
  const values = initialValues(state.fields)
  const lists = initialLists(state.fields)
  const resets = new Set<string>()
  for (const field of state.fields) {
    if (isListField(field)) {
      const edit = draft?.lists[field.key]
      if (edit?.kind === 'replace') lists[field.key] = [...edit.items]
      if (edit?.kind === 'reset') resets.add(field.key)
    } else {
      const edit = draft?.values[field.key]
      if (edit?.kind === 'replace') values[field.key] = edit.value
      if (edit?.kind === 'clear') values[field.key] = field.secret ? null : ''
    }
  }
  return { label: draft?.label ?? state.label, values, lists, resets }
}

/** The request that saves `draft` for `state`'s connection; null when it changes nothing. */
export function saveRequestOf(
  state: ProviderConfigConnectionStateDto,
  draft: SourceDraft
): ProviderConfigSaveRequestDto | null {
  const values = state.fields.flatMap((field): ProviderConfigSaveRequestDto['values'] => {
    const edit = isListField(field) ? draft.lists[field.key] : draft.values[field.key]
    return edit === undefined ? [] : [{ key: field.key, value: requestValue(edit) }]
  })
  if (values.length === 0 && draft.label === null) return null
  return {
    connection: state.connection,
    ...(draft.label === null ? {} : { label: draft.label }),
    values,
    basedOn: draft.basedOn,
  }
}

/** An edit as the request spells it: `null` clears a value or resets a list. */
function requestValue(
  edit: Exclude<ValueEdit | ListEdit, { kind: 'untouched' }>
): string | string[] | null {
  switch (edit.kind) {
    case 'clear':
    case 'reset':
      return null
    case 'replace':
      return 'value' in edit ? edit.value : [...edit.items]
  }
}

function initialValueFor(field: SingleField): string {
  const value = field.secret ? '' : (field.value ?? '')
  if (field.valueType !== 'bool') return value
  const normalized = value.trim().toLowerCase()
  return normalized === 'true' || normalized === 'false' ? normalized : value
}

/** Trimmed, without blank rows: what the backend would store. */
function cleanItems(items: readonly string[]): string[] {
  return items.map(item => item.trim()).filter(item => item !== '')
}

function sameItems(a: readonly string[], b: readonly string[]): boolean {
  return a.length === b.length && a.every((item, index) => item === b[index])
}
