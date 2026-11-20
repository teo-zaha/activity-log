import { describe, expect, it } from 'vitest'
import type {
  ProviderConfigConnectionStateDto,
  ProviderConfigFieldStateDto,
} from '../../../generated/activity'
import { type DraftForm, draftOf, formOf, sameDraft, saveRequestOf } from './source-draft-request'

const fields: ProviderConfigFieldStateDto[] = [
  {
    key: 'baseUrl',
    label: 'Base URL',
    required: true,
    shape: 'single',
    valueType: 'url',
    secret: false,
    isSet: true,
    value: 'https://jira.example.org',
  },
  {
    key: 'apiToken',
    label: 'API token',
    required: true,
    shape: 'single',
    valueType: 'password',
    secret: true,
    isSet: true,
  },
  {
    key: 'projects',
    label: 'Projects',
    required: false,
    shape: 'list',
    itemType: 'string',
    isSet: true,
    items: ['AL'],
  },
]

const work: ProviderConfigConnectionStateDto = {
  connection: { source: 'jira', name: 'work' },
  label: 'Work',
  fields,
}

const form = (changes: Partial<DraftForm> = {}): DraftForm => ({ ...formOf(work), ...changes })

describe('drafts and the requests they make', () => {
  it('asks for nothing while the fields show what is saved', () => {
    expect(draftOf(work, form())).toEqual({ label: null, values: {}, lists: {} })
    expect(saveRequestOf(work, draftOf(work, form()))).toBeNull()
  })

  it('keeps only what the user changed, and sends that for the draft’s connection', () => {
    const draft = draftOf(
      work,
      form({
        label: ' Office ',
        values: { ...formOf(work).values, apiToken: null },
        lists: { projects: ['AL', ' ', 'OPS '] },
      })
    )

    expect(draft).toEqual({
      label: 'Office',
      values: { apiToken: { kind: 'clear' } },
      lists: { projects: { kind: 'replace', items: ['AL', 'OPS'] } },
    })
    expect(saveRequestOf(work, draft)).toEqual({
      connection: { source: 'jira', name: 'work' },
      label: 'Office',
      values: [
        { key: 'apiToken', value: null },
        { key: 'projects', value: ['AL', 'OPS'] },
      ],
    })
  })

  it('leaves a field changed elsewhere alone when the draft did not touch it', () => {
    const draft = draftOf(work, form({ values: { ...formOf(work).values, apiToken: 'new-token' } }))
    // Another writer changed the URL; the draft only replaced the token.
    const changed: ProviderConfigConnectionStateDto = {
      ...work,
      fields: fields.map(field =>
        field.key === 'baseUrl' ? { ...field, value: 'https://jira2.example.org' } : field
      ),
    }

    expect(formOf(changed, draft).values['baseUrl']).toBe('https://jira2.example.org')
    expect(saveRequestOf(changed, draft)?.values).toEqual([{ key: 'apiToken', value: 'new-token' }])
  })

  it('resets a configured list to its default', () => {
    const draft = draftOf(work, form({ resets: new Set(['projects']) }))

    expect(draft.lists).toEqual({ projects: { kind: 'reset' } })
    expect(formOf(work, draft).resets).toEqual(new Set(['projects']))
    expect(saveRequestOf(work, draft)?.values).toEqual([{ key: 'projects', value: null }])
  })

  it('shows a draft’s edits over the saved settings', () => {
    const draft = draftOf(
      work,
      form({ label: 'Office', values: { ...formOf(work).values, baseUrl: '' } })
    )

    expect(formOf(work, draft)).toMatchObject({
      label: 'Office',
      values: { baseUrl: '', apiToken: '' },
      lists: { projects: ['AL'] },
    })
  })

  it('takes a saved value with outer spaces, shown and not edited, as untouched', () => {
    const padded: ProviderConfigConnectionStateDto = {
      ...work,
      fields: fields.map(field =>
        field.key === 'baseUrl' && field.shape === 'single'
          ? { ...field, value: ' https://jira.example.org ' }
          : field
      ),
    }

    expect(saveRequestOf(padded, draftOf(padded, formOf(padded)))).toBeNull()
  })

  it('tells drafts apart by what they ask, not by when they were made', () => {
    const draft = draftOf(
      work,
      form({ label: 'Office', values: { ...formOf(work).values, baseUrl: '' } })
    )

    expect(sameDraft(draft, draftOf(work, formOf(work, draft)))).toBe(true)
    expect(sameDraft(draft, { ...draft, label: 'Home' })).toBe(false)
    expect(sameDraft(draft, { ...draft, lists: { projects: { kind: 'reset' } } })).toBe(false)
  })
})
