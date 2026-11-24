import { describe, expect, it } from 'vitest'
import { figmaEntries } from '../../testing/activity-fixtures'
import { createEntryAdapter } from '../provider-adapter'

describe('Figma adapter', () => {
  it('names a saved version and includes its description in details and search', () => {
    const adapter = createEntryAdapter(figmaEntries.versionSaved)

    expect(adapter.title).toBe('Checkout')
    expect(adapter.meta).toBe('Saved version “Checkout v2”')
    expect(adapter.badge).toEqual({ source: 'figma', label: 'Figma', icon: 'draw' })
    expect(adapter.detailFields).toEqual([
      { label: 'File', value: 'Checkout' },
      { label: 'Kind', value: 'Design' },
      { label: 'Version', value: 'Checkout v2' },
      { label: 'Description', value: 'Updated form labels' },
    ])
    expect(adapter.searchTerms).toEqual(
      expect.arrayContaining(['Checkout', 'Checkout v2', 'Updated form labels'])
    )
  })

  it('shows an autosave as an edit with only the file when the editor is absent', () => {
    const adapter = createEntryAdapter(figmaEntries.edited)

    expect(adapter.title).toBe('Checkout')
    expect(adapter.meta).toBe('Edited')
    expect(adapter.detailFields).toEqual([{ label: 'File', value: 'Checkout' }])
  })

  it('shows a comment, its resolution, and its searchable text', () => {
    const adapter = createEntryAdapter(figmaEntries.commented)

    expect(adapter.title).toBe('Checkout')
    expect(adapter.meta).toBe('Commented')
    expect(adapter.detailFields).toContainEqual({ label: 'Comment', value: 'Looks good' })
    expect(adapter.detailFields).toContainEqual({ label: 'Resolved', value: 'Yes' })
    expect(adapter.searchTerms).toContain('Looks good')
    expect(adapter.detailLink).toEqual({
      label: 'Open in Figma',
      url: figmaEntries.commented.fileUrl,
    })
  })

  it('shows a reply with an explicit fallback for missing comment text', () => {
    const adapter = createEntryAdapter(figmaEntries.replied)

    expect(adapter.title).toBe('Checkout')
    expect(adapter.meta).toBe('Replied')
    expect(adapter.detailFields).toEqual([
      { label: 'File', value: 'Checkout' },
      { label: 'Kind', value: 'Design' },
      { label: 'Comment', value: '(No comment text)' },
    ])
    expect(adapter.searchTerms).not.toContain('(No comment text)')
  })
})
