import { describe, expect, it } from 'vitest'
import {
  CHARACTER_KEYS,
  type ChordKey,
  type Os,
  ariaChord,
  isCharacterKeys,
  menuKeyOf,
} from '../utils/key-chord'
import { NAV_GROUPS } from './nav'
import {
  type Build,
  type CharacterKeys,
  type ChordsHere,
  SHORTCUT_IDS,
  SHORTCUT_SCOPES,
  SHORTCUTS,
  type ShortcutEntry,
  type ShortcutId,
  VIEW_SHORTCUTS,
  chordsIn,
  isCharacterChord,
  isMenuAccelerator,
  resolveChord,
  shortcut,
} from './shortcuts'

const OSES: readonly Os[] = ['mac', 'other']
const BUILDS: readonly Build[] = ['desktop', 'browser']
const SETTINGS: readonly CharacterKeys[] = ['on', 'off']

/** Every place keys are resolved in: platform, build and the single-key setting. */
const EVERYWHERE: readonly ChordsHere[] = OSES.flatMap(os =>
  BUILDS.flatMap(build => SETTINGS.map(characterKeys => ({ os, build, characterKeys })))
)

const desktop = (os: Os): ChordsHere => ({ os, build: 'desktop', characterKeys: 'on' })
const browser = (os: Os): ChordsHere => ({ os, build: 'browser', characterKeys: 'on' })

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

/** The browser leaves these to itself: its own Back does what `app.back` does. */
const BROWSER_DOES_ITSELF = new Set<ShortcutId>(['app.back'])
/** Only the menu and the header's buttons: a single key would write a file or the clipboard. */
const MENU_ONLY = new Set<ShortcutId>(['app.export-markdown', 'app.export-json', 'app.copy-link'])
/**
 * Keys every common layout types without AltGr or ⌥: letters by their physical key, and these
 * characters, with Shift where the layout needs it (`/` is Shift+7 on a German layout). `[` is not
 * among them: it needs ⌥ or AltGr on German and Nordic layouts, so it is only ever ⌘[.
 */
const TYPABLE = new Set<string>([
  '/',
  '?',
  ',',
  '1',
  '2',
  '3',
  '4',
  ...'abcdefghijklmnopqrstuvwxyz',
])

const keysOf = (entry: ShortcutEntry, here: ChordsHere): string[] =>
  chordsIn(entry, here).map(chord => chord.keys)

describe('the shortcut registry', () => {
  it('has each id once', () => {
    expect(new Set(SHORTCUT_IDS).size).toBe(SHORTCUT_IDS.length)
    expect(new Set(SHORTCUTS.map(entry => entry.id)).size).toBe(SHORTCUT_IDS.length)
  })

  it('puts every entry in a scope the help has a section for', () => {
    for (const entry of SHORTCUTS) expect(SHORTCUT_SCOPES).toContain(entry.scope)
  })

  it('gives no two entries of one scope the same chord anywhere', () => {
    for (const here of EVERYWHERE) {
      const seen = new Set<string>()
      for (const entry of SHORTCUTS) {
        for (const chord of chordsIn(entry, here)) {
          const key = `${entry.scope} ${chord.keys}`
          expect(seen.has(key), `${key} in ${JSON.stringify(here)}`).toBe(false)
          seen.add(key)
        }
      }
    }
  })

  it('lets a scope’s entry share a global entry’s chord only where it stands for it', () => {
    for (const here of EVERYWHERE) {
      const global = SHORTCUTS.filter(entry => entry.scope === 'global')
      for (const scoped of SHORTCUTS.filter(entry => entry.scope !== 'global')) {
        for (const other of global) {
          const shared = keysOf(scoped, here).filter(keys => keysOf(other, here).includes(keys))
          if (shared.length > 0) expect(scoped.standsFor, `${scoped.id} ${other.id}`).toBe(other.id)
        }
      }
    }
  })

  it('lets only a scope’s entry stand for another, and only for a global one', () => {
    for (const entry of SHORTCUTS) {
      if (entry.standsFor === undefined) continue
      expect(entry.scope, entry.id).not.toBe('global')
      expect(shortcut(entry.standsFor).scope, entry.id).toBe('global')
    }
    expect(shortcut('timeline.filter').standsFor).toBe('app.filter')
  })

  it('reads a key alone as a character key exactly when it types a character', () => {
    const named: readonly ChordKey[] = ['escape', 'f5', 'arrowleft']
    for (const key of [...CHARACTER_KEYS, ...named]) {
      // `aria-keyshortcuts` names a key that types by its character, any other key by its name.
      expect(isCharacterKeys(key), key).toBe(ariaChord(key, 'mac').length === 1)
    }
    expect(isCharacterKeys('mod+r')).toBe(false)
  })

  it('makes no character key a menu accelerator, and only modified chords without a text-editing meaning', () => {
    for (const chord of SHORTCUTS.flatMap(entry => entry.chords)) {
      if (isCharacterChord(chord)) expect(isMenuAccelerator(chord), chord.keys).toBe(false)
      if (isMenuAccelerator(chord)) expect(chord.keys.startsWith('mod+'), chord.keys).toBe(true)
    }
    for (const os of OSES) {
      for (const chord of SHORTCUTS.flatMap(entry => chordsIn(entry, desktop(os)))) {
        if (isMenuAccelerator(chord))
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
    for (const os of OSES) {
      const byChord = new Map<string, ShortcutEntry['menu']>()
      for (const entry of SHORTCUTS) {
        for (const chord of chordsIn(entry, desktop(os)).filter(isMenuAccelerator)) {
          const submenu = byChord.get(chord.keys)
          if (submenu !== undefined) expect(entry.menu).toBe(submenu)
          byChord.set(chord.keys, entry.menu)
        }
      }
    }
    const menuLabels = SHORTCUTS.filter(entry => entry.menu).map(e => `${e.menu} ${e.label}`)
    expect(new Set(menuLabels).size).toBe(menuLabels.length)
  })

  it('gives each menu accelerator a menu item and a key the menu has', () => {
    for (const entry of SHORTCUTS) {
      for (const chord of entry.chords.filter(isMenuAccelerator)) {
        expect(entry.menu, `${entry.id} ${chord.keys}`).toBeDefined()
        expect(menuKeyOf(chord.keys), `${entry.id} ${chord.keys}`).not.toBeNull()
      }
    }
  })

  it('shows each view of the navigation with ⌘1 to ⌘4 and 1 to 4, listed in View', () => {
    const views = NAV_GROUPS.flatMap(group => group.items).map(item => VIEW_SHORTCUTS[item.id])
    expect(views.map(id => keysOf(shortcut(id), desktop('mac')))).toEqual([
      ['mod+1', '1'],
      ['mod+2', '2'],
      ['mod+3', '3'],
      ['mod+4', '4'],
    ])
    for (const id of views) expect(shortcut(id).menu).toBe('view')
  })

  it('leaves the browser its modified keys: there every chord is a character key or Escape', () => {
    for (const os of OSES) {
      for (const chord of SHORTCUTS.flatMap(entry => chordsIn(entry, browser(os)))) {
        expect(isCharacterChord(chord) || chord.keys === 'escape', chord.keys).toBe(true)
      }
    }
  })

  it('gives every command a key in the browser, but Back and the menu-only ones', () => {
    for (const os of OSES) {
      for (const entry of SHORTCUTS) {
        const expected = !BROWSER_DOES_ITSELF.has(entry.id) && !MENU_ONLY.has(entry.id)
        expect(keysOf(entry, browser(os)).length > 0, `${entry.id} on ${os}`).toBe(expected)
      }
    }
  })

  it('gives the desktop app every key the browser has', () => {
    for (const os of OSES) {
      for (const characterKeys of SETTINGS) {
        for (const entry of SHORTCUTS) {
          const inApp = keysOf(entry, { os, build: 'desktop', characterKeys })
          for (const keys of keysOf(entry, { os, build: 'browser', characterKeys }))
            expect(inApp, `${entry.id} on ${os}`).toContain(keys)
        }
      }
    }
  })

  it('lists a command’s modified chords before its character key, so the app’s tag shows ⌘R', () => {
    for (const entry of SHORTCUTS) {
      const kinds = entry.chords.map(isCharacterChord)
      expect(kinds, entry.id).toEqual([...kinds].sort((a, b) => Number(a) - Number(b)))
    }
  })

  it('takes every character key away with the single-key setting, and nothing else', () => {
    for (const os of OSES) {
      for (const build of BUILDS) {
        for (const entry of SHORTCUTS) {
          const on = chordsIn(entry, { os, build, characterKeys: 'on' })
          const off = chordsIn(entry, { os, build, characterKeys: 'off' })
          expect(off, `${entry.id} on ${os} ${build}`).toEqual(
            on.filter(chord => !isCharacterChord(chord))
          )
        }
      }
    }
  })

  it('uses only character keys every common layout types', () => {
    for (const chord of SHORTCUTS.flatMap(entry => entry.chords).filter(isCharacterChord)) {
      expect(TYPABLE.has(chord.keys), chord.keys).toBe(true)
    }
  })

  it('collects with ⌘R on macOS, Ctrl+R or F5 elsewhere, also while typing, and with R', () => {
    const collect = shortcut('app.collect')
    expect(keysOf(collect, desktop('mac'))).toEqual(['mod+r', 'r'])
    expect(keysOf(collect, desktop('other'))).toEqual(['mod+r', 'f5', 'r'])
    expect(keysOf(collect, browser('other'))).toEqual(['r'])
    for (const chord of collect.chords) {
      if (!isCharacterChord(chord)) expect(chord.whileTyping, chord.keys).toBe(true)
    }
  })

  it('goes back with ⌘[ on macOS and Alt+← elsewhere, not while typing, and not in a browser', () => {
    const back = shortcut('app.back')
    expect(keysOf(back, desktop('mac'))).toEqual(['mod+['])
    expect(keysOf(back, desktop('other'))).toEqual(['alt+arrowleft'])
    expect(keysOf(back, browser('mac'))).toEqual([])
    for (const chord of back.chords) {
      if (!isCharacterChord(chord)) expect(chord.whileTyping, chord.keys).toBe(false)
    }
  })

  it('steps the calendar and the details with P and N, and goes to today with T', () => {
    const here = browser('mac')
    expect(keysOf(shortcut('calendar.previous'), here)).toEqual(['p'])
    expect(keysOf(shortcut('calendar.next'), here)).toEqual(['n'])
    expect(keysOf(shortcut('calendar.today'), here)).toEqual(['t'])
    expect(keysOf(shortcut('details.previous'), here)).toEqual(['p'])
    expect(keysOf(shortcut('details.next'), here)).toEqual(['n'])
    for (const id of ['calendar.previous', 'calendar.next', 'calendar.today'] as const)
      expect(shortcut(id).scope).toBe('calendar')
    for (const id of ['details.previous', 'details.next'] as const) {
      expect(shortcut(id).scope).toBe('details')
      expect(shortcut(id).menu).toBeUndefined()
    }
  })
})

describe('resolveChord', () => {
  const entry = (fields: Pick<ShortcutEntry, 'id' | 'scope'>, keys: 'mod+f'): ShortcutEntry => ({
    ...fields,
    label: fields.id,
    chords: [{ keys, whileTyping: false, menuAccelerator: false }],
  })
  // Two entries with one chord, as Work items' search will share ⌘F with the Timeline's filter.
  const global = entry({ id: 'app.filter', scope: 'global' }, 'mod+f')
  const timeline = entry({ id: 'timeline.filter', scope: 'timeline' }, 'mod+f')
  const app = desktop('mac')

  it('lets the scope on screen win over a global entry with the same chord', () => {
    expect(resolveChord([global, timeline], { ...app, scope: 'timeline' }, 'mod+f')?.entry).toBe(
      timeline
    )
    expect(resolveChord([timeline, global], { ...app, scope: 'timeline' }, 'mod+f')?.entry).toBe(
      timeline
    )
  })

  it('leaves a scope that is not on screen out', () => {
    expect(resolveChord([global, timeline], { ...app, scope: 'global' }, 'mod+f')?.entry).toBe(
      global
    )
    expect(resolveChord([timeline], { ...app, scope: 'global' }, 'mod+f')).toBeNull()
  })

  it('matches only the chords of the platform, the build and the single-key setting', () => {
    const at = (here: ChordsHere, keys: string): ShortcutId | null =>
      resolveChord(SHORTCUTS, { ...here, scope: 'global' }, keys)?.entry.id ?? null
    expect(at(desktop('mac'), 'f5')).toBeNull()
    expect(at(desktop('other'), 'f5')).toBe('app.collect')
    expect(at(browser('other'), 'mod+r')).toBeNull()
    expect(at(browser('other'), 'r')).toBe('app.collect')
    expect(at({ ...browser('other'), characterKeys: 'off' }, 'r')).toBeNull()
    expect(at({ ...desktop('other'), characterKeys: 'off' }, 'mod+r')).toBe('app.collect')
  })

  it('reads P and N by the scope on screen: the calendar’s, the details’, or none', () => {
    const at = (scope: 'calendar' | 'details' | 'timeline', keys: string): ShortcutId | null =>
      resolveChord(SHORTCUTS, { ...browser('mac'), scope }, keys)?.entry.id ?? null
    expect(at('calendar', 'n')).toBe('calendar.next')
    expect(at('details', 'n')).toBe('details.next')
    expect(at('details', 't')).toBeNull()
    expect(at('timeline', 'p')).toBeNull()
  })
})
