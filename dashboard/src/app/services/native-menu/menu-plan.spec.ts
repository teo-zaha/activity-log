import { describe, expect, it } from 'vitest'
import { SHORTCUTS, type ShortcutEntry, isCharacterChord } from '../../models/shortcuts'
import { menuPlanOf, menuSpecOf, sameState } from './menu-plan'

/** Each section as `submenu: title (accelerator)`, the way the menu bar reads. */
function outline(entries: readonly ShortcutEntry[]): string[][] {
  return menuPlanOf(entries, 'mac').map(section =>
    section.items.map(
      item =>
        `${section.submenu}: ${item.entries.map(e => e.label).join(' / ')}` +
        (item.accelerator === null ? '' : ` (${item.accelerator})`)
    )
  )
}

describe('menuPlanOf', () => {
  it('lists the table’s menu entries per submenu, the views as the navigation groups them', () => {
    expect(outline(SHORTCUTS)).toEqual([
      ['app: Settings… (comma)'],
      ['file: Collect (r)', 'file: Export Markdown…', 'file: Export JSON…', 'file: Copy Link'],
      ['edit: Filter Activity… (f)'],
      [
        'view: Timeline (digit1)',
        'view: Day (digit2)',
        'view: Work week (digit3)',
        'view: Week (digit4)',
      ],
      ['view: Back (bracketLeft)'],
      ['help: Keyboard Shortcuts'],
    ])
  })

  it('is the same with the single keys as without: they are the page’s, never the menu’s', () => {
    const withoutSingleKeys = SHORTCUTS.map(entry => ({
      ...entry,
      chords: entry.chords.filter(chord => !isCharacterChord(chord)),
    }))
    // What the menu shows and runs: the items, their accelerators and actions, not the chords.
    const shown = (entries: readonly ShortcutEntry[], os: 'mac' | 'other'): unknown =>
      menuPlanOf(entries, os).map(section => ({
        submenu: section.submenu,
        items: section.items.map(({ entries: of, ...item }) => ({
          ...item,
          ids: of.map(e => e.id),
        })),
      }))
    for (const os of ['mac', 'other'] as const) {
      expect(shown(SHORTCUTS, os)).toEqual(shown(withoutSingleKeys, os))
    }
  })

  it('checks the views, runs a shared accelerator by its chord and the others by their entry', () => {
    const items = menuPlanOf(SHORTCUTS, 'mac').flatMap(section => section.items)
    const byLabel = new Map(items.map(item => [item.entries[0].label, item]))

    expect(byLabel.get('Day')?.view).toBe('day')
    expect(byLabel.get('Back')?.view).toBeNull()
    expect(byLabel.get('Filter Activity…')?.action).toEqual({ kind: 'chord', keys: 'mod+f' })
    expect(byLabel.get('Copy Link')?.action).toEqual({ kind: 'entry', id: 'app.copy-link' })
    // The `?` stays the page's: the help's item has no accelerator.
    expect(byLabel.get('Keyboard Shortcuts')?.accelerator).toBeNull()
    expect(new Set(items.map(item => item.key)).size).toBe(items.length)
  })

  it('makes entries that share an accelerator one item, as Work items will share ⌘F', () => {
    const filter: ShortcutEntry = {
      id: 'app.filter',
      label: 'Filter Activity…',
      scope: 'global',
      menu: 'edit',
      chords: [{ keys: 'mod+f', whileTyping: true, menuAccelerator: true }],
    }
    const search: ShortcutEntry = { ...filter, id: 'timeline.filter', scope: 'timeline' }

    const [section] = menuPlanOf([filter, search], 'mac')

    expect(section?.items).toHaveLength(1)
    expect(section?.items[0]?.entries).toEqual([filter, search])
    expect(section?.items[0]?.action).toEqual({ kind: 'chord', keys: 'mod+f' })
  })

  it('refuses an accelerator the menu has no key for, so no menu takes the page’s keys', () => {
    const odd: ShortcutEntry = {
      id: 'layer.close',
      label: 'Close',
      scope: 'global',
      menu: 'window',
      chords: [{ keys: 'mod+escape', whileTyping: true, menuAccelerator: true }],
    }
    expect(() => menuPlanOf([odd], 'mac')).toThrow()
  })
})

describe('menuSpecOf', () => {
  it('sends each item’s key, accelerator and state, and no accelerator where it has none', () => {
    const plan = menuPlanOf(SHORTCUTS, 'mac')
    const spec = menuSpecOf(plan, item => ({
      kind: 'command',
      title: item.entries[0].label,
      enabled: true,
    }))

    expect(spec.sections.map(section => section.submenu)).toEqual([
      'app',
      'file',
      'edit',
      'view',
      'view',
      'help',
    ])
    expect(spec.sections[1]?.items[0]).toEqual({
      key: 1,
      accelerator: 'r',
      state: { kind: 'command', title: 'Collect', enabled: true },
    })
    expect(spec.sections[1]?.items[1]).not.toHaveProperty('accelerator')
  })
})

describe('sameState', () => {
  it('tells states apart by everything they show', () => {
    const day = { kind: 'check', title: 'Day', enabled: true, checked: false } as const
    expect(sameState(day, { ...day })).toBe(true)
    expect(sameState(day, { ...day, checked: true })).toBe(false)
    expect(sameState(day, { kind: 'command', title: 'Day', enabled: true })).toBe(false)
  })
})
