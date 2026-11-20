import { describe, expect, it } from 'vitest'
import type { Os } from '../utils/key-chord'
import {
  type Build,
  SHORTCUT_IDS,
  SHORTCUT_SCOPES,
  SHORTCUTS,
  type ShortcutEntry,
  chordsIn,
  resolveChord,
} from './shortcuts'

const PLATFORMS: readonly { os: Os; build: Build }[] = [
  { os: 'mac', build: 'desktop' },
  { os: 'other', build: 'desktop' },
  { os: 'mac', build: 'browser' },
  { os: 'other', build: 'browser' },
]

/**
 * Chords that edit text or move the caret in a field, per platform, as `chordOf` writes them: a
 * menu accelerator takes its key before the page, so none of these may be one.
 */
const TEXT_EDITING: Readonly<Record<Os, ReadonlySet<string>>> = {
  mac: new Set(
    ['arrowleft', 'arrowright', 'arrowup', 'arrowdown', 'backspace', 'delete']
      .flatMap(key => [`mod+${key}`, `alt+${key}`, `mod+shift+${key}`, `alt+shift+${key}`])
      .concat(['mod+a', 'mod+c', 'mod+v', 'mod+x', 'mod+z', 'mod+shift+z', 'mod+alt+shift+v'])
  ),
  other: new Set(
    ['arrowleft', 'arrowright', 'arrowup', 'arrowdown', 'backspace', 'delete', 'home', 'end']
      .flatMap(key => [`mod+${key}`, `mod+shift+${key}`])
      .concat(['mod+a', 'mod+c', 'mod+v', 'mod+x', 'mod+z', 'mod+y', 'mod+shift+v', 'mod+shift+z'])
  ),
}

/** A key that types a character: one character, with no modifier. */
const typesCharacter = (keys: string): boolean => keys.length === 1

describe('the shortcut registry', () => {
  it('has each id once', () => {
    expect(new Set(SHORTCUT_IDS).size).toBe(SHORTCUT_IDS.length)
    expect(new Set(SHORTCUTS.map(entry => entry.id)).size).toBe(SHORTCUT_IDS.length)
  })

  it('puts every entry in a scope the help has a section for', () => {
    for (const entry of SHORTCUTS) expect(SHORTCUT_SCOPES).toContain(entry.scope)
  })

  it('gives no two entries of one scope the same chord on one platform', () => {
    for (const { os, build } of PLATFORMS) {
      const seen = new Set<string>()
      for (const entry of SHORTCUTS) {
        for (const chord of chordsIn(entry, os, build)) {
          const key = `${entry.scope} ${chord.keys}`
          expect(seen.has(key), `${key} on ${os} ${build}`).toBe(false)
          seen.add(key)
        }
      }
    }
  })

  it('never fires a character key while typing or takes it as a menu accelerator', () => {
    for (const chord of SHORTCUTS.flatMap(entry => entry.chords)) {
      if (typesCharacter(chord.keys)) {
        expect(chord.whileTyping, chord.keys).toBe(false)
        expect(chord.menuAccelerator, chord.keys).toBe(false)
      }
    }
  })

  it('makes only modified chords without a text-editing meaning menu accelerators', () => {
    const accelerators = SHORTCUTS.flatMap(entry => entry.chords).filter(c => c.menuAccelerator)
    for (const chord of accelerators) {
      expect(chord.keys.startsWith('mod+'), chord.keys).toBe(true)
    }
    for (const os of ['mac', 'other'] as const) {
      for (const chord of SHORTCUTS.flatMap(entry => chordsIn(entry, os, 'desktop'))) {
        if (chord.menuAccelerator)
          expect(TEXT_EDITING[os].has(chord.keys), `${chord.keys} on ${os}`).toBe(false)
      }
    }
  })

  it('knows the text-editing chords it checks accelerators against', () => {
    // ⌘← goes to the line's start on macOS; Alt+← moves by word there, Ctrl+← elsewhere.
    expect(TEXT_EDITING.mac.has('mod+arrowleft')).toBe(true)
    expect(TEXT_EDITING.mac.has('alt+arrowleft')).toBe(true)
    expect(TEXT_EDITING.other.has('mod+arrowleft')).toBe(true)
    expect(TEXT_EDITING.mac.has('mod+[')).toBe(false)
  })

  it('lists entries that share an accelerator in one submenu, under distinct labels', () => {
    for (const os of ['mac', 'other'] as const) {
      const byChord = new Map<string, ShortcutEntry['menu']>()
      for (const entry of SHORTCUTS) {
        for (const chord of chordsIn(entry, os, 'desktop').filter(c => c.menuAccelerator)) {
          const submenu = byChord.get(chord.keys)
          if (submenu !== undefined) expect(entry.menu).toBe(submenu)
          byChord.set(chord.keys, entry.menu)
        }
      }
    }
    const menuLabels = SHORTCUTS.filter(entry => entry.menu).map(e => `${e.menu} ${e.label}`)
    expect(new Set(menuLabels).size).toBe(menuLabels.length)
  })

  it('leaves the browser only `/` and `?`, and the layers their Escape', () => {
    for (const os of ['mac', 'other'] as const) {
      const keys = SHORTCUTS.flatMap(entry => chordsIn(entry, os, 'browser').map(c => c.keys))
      expect(new Set(keys)).toEqual(new Set(['/', '?', 'escape']))
    }
  })

  it('collects with ⌘R on macOS and with Ctrl+R or F5 elsewhere, also while typing', () => {
    const collect = SHORTCUTS.find(entry => entry.id === 'app.collect')
    if (!collect) throw new Error('no app.collect')
    expect(chordsIn(collect, 'mac', 'desktop').map(c => c.keys)).toEqual(['mod+r'])
    expect(chordsIn(collect, 'other', 'desktop').map(c => c.keys)).toEqual(['mod+r', 'f5'])
    expect(collect.chords.every(chord => chord.whileTyping)).toBe(true)
  })

  it('goes back with ⌘[ on macOS and Alt+← elsewhere, not while typing', () => {
    const back = SHORTCUTS.find(entry => entry.id === 'app.back')
    if (!back) throw new Error('no app.back')
    expect(chordsIn(back, 'mac', 'desktop').map(c => c.keys)).toEqual(['mod+['])
    expect(chordsIn(back, 'other', 'desktop').map(c => c.keys)).toEqual(['alt+arrowleft'])
    expect(back.chords.some(chord => chord.whileTyping)).toBe(false)
  })
})

describe('resolveChord', () => {
  const entry = (
    fields: Pick<ShortcutEntry, 'id' | 'scope'>,
    keys: '/' | 'mod+f'
  ): ShortcutEntry => ({
    ...fields,
    label: fields.id,
    chords: [{ keys, whileTyping: false, menuAccelerator: false }],
  })
  // Two entries with one chord, as Work items' search will share ⌘F with the Timeline's filter.
  const global = entry({ id: 'app.filter', scope: 'global' }, 'mod+f')
  const timeline = entry({ id: 'timeline.filter', scope: 'timeline' }, 'mod+f')
  const desktop = { os: 'mac', build: 'desktop' } as const

  it('lets the scope on screen win over a global entry with the same chord', () => {
    expect(
      resolveChord([global, timeline], { ...desktop, scope: 'timeline' }, 'mod+f')?.entry
    ).toBe(timeline)
    expect(
      resolveChord([timeline, global], { ...desktop, scope: 'timeline' }, 'mod+f')?.entry
    ).toBe(timeline)
  })

  it('leaves a scope that is not on screen out', () => {
    expect(resolveChord([global, timeline], { ...desktop, scope: 'global' }, 'mod+f')?.entry).toBe(
      global
    )
    expect(resolveChord([timeline], { ...desktop, scope: 'global' }, 'mod+f')).toBeNull()
  })

  it('matches only the chords of the platform and the build', () => {
    expect(
      resolveChord(SHORTCUTS, { os: 'mac', build: 'desktop', scope: 'global' }, 'f5')
    ).toBeNull()
    expect(
      resolveChord(SHORTCUTS, { os: 'other', build: 'desktop', scope: 'global' }, 'f5')?.entry.id
    ).toBe('app.collect')
    expect(
      resolveChord(SHORTCUTS, { os: 'other', build: 'browser', scope: 'global' }, 'mod+r')
    ).toBeNull()
  })
})
