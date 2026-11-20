import type { ChordKeys, Os } from '../utils/key-chord'

/**
 * Where an entry's keys work: `global` everywhere, or in the view whose route declares the
 * scope (`data.shortcutScope`). A view the router keeps while another shows, or that the phone's
 * Settings tab hides, holds no keys then (`ShortcutService`).
 */
export const SHORTCUT_SCOPES = ['global', 'timeline'] as const
export type ShortcutScope = (typeof SHORTCUT_SCOPES)[number]

/** The macOS menu's submenus an entry is listed in (the menu is built from this table). */
export type MenuSubmenu = 'app' | 'file' | 'edit' | 'view' | 'window' | 'help'

/** Where the dashboard runs: the desktop app, or a browser tab whose keys are the browser's. */
export type Build = 'desktop' | 'browser'

export interface Chord {
  readonly keys: ChordKeys
  /** Absent: every platform. */
  readonly os?: Os
  /** Absent: wherever the entry is. `desktop`: not in the browser, whose own key it is. */
  readonly where?: 'desktop'
  /** Fires while focus is in an editable field. Never for a chord that types a character. */
  readonly whileTyping: boolean
  /**
   * On macOS the menu's accelerator, once the menu is built from this table: only a modified
   * chord with no text-editing meaning there, since the menu takes the key before the page.
   */
  readonly menuAccelerator: boolean
}

export interface ShortcutEntry {
  readonly id: ShortcutId
  /** Verbatim in the help, beside a key tag and in the macOS menu. */
  readonly label: string
  readonly scope: ShortcutScope
  readonly chords: readonly Chord[]
  readonly menu?: MenuSubmenu
}

/** Every shortcut, in the help's order. */
export const SHORTCUT_IDS = [
  'app.collect',
  'app.filter',
  'timeline.filter',
  'app.back',
  'layer.close',
  'app.help',
] as const

export type ShortcutId = (typeof SHORTCUT_IDS)[number]

const ENTRIES: { readonly [Id in ShortcutId]: Omit<ShortcutEntry, 'id'> } = {
  // Ctrl+R and F5 reload a browser tab; in the desktop app they collect (WebView2's own browser
  // keys are off, `desktop/src/browser_keys/`).
  'app.collect': {
    label: 'Collect',
    scope: 'global',
    menu: 'file',
    chords: [
      { keys: 'mod+r', where: 'desktop', whileTyping: true, menuAccelerator: true },
      { keys: 'f5', os: 'other', where: 'desktop', whileTyping: true, menuAccelerator: false },
    ],
  },
  // A browser keeps its own find.
  'app.filter': {
    label: 'Filter Activity…',
    scope: 'global',
    menu: 'edit',
    chords: [{ keys: 'mod+f', where: 'desktop', whileTyping: true, menuAccelerator: true }],
  },
  'timeline.filter': {
    label: 'Filter activity',
    scope: 'timeline',
    chords: [{ keys: '/', whileTyping: false, menuAccelerator: false }],
  },
  // Alt+← moves by word in a text field on macOS and is never a menu accelerator.
  'app.back': {
    label: 'Back',
    scope: 'global',
    menu: 'view',
    chords: [
      { keys: 'mod+[', os: 'mac', where: 'desktop', whileTyping: false, menuAccelerator: true },
      {
        keys: 'alt+arrowleft',
        os: 'other',
        where: 'desktop',
        whileTyping: false,
        menuAccelerator: false,
      },
    ],
  },
  // A layer's own binding: each one binds it while it is open, the top one closes.
  'layer.close': {
    label: 'Close details or settings',
    scope: 'global',
    chords: [{ keys: 'escape', whileTyping: true, menuAccelerator: false }],
  },
  'app.help': {
    label: 'Keyboard Shortcuts',
    scope: 'global',
    menu: 'help',
    chords: [{ keys: '?', whileTyping: false, menuAccelerator: false }],
  },
}

export const SHORTCUTS: readonly ShortcutEntry[] = SHORTCUT_IDS.map(id => shortcut(id))

export function shortcut(id: ShortcutId): ShortcutEntry {
  return { id, ...ENTRIES[id] }
}

/** The chords of `entry` that work on `os` in `build`, first the one a key tag shows. */
export function chordsIn(entry: ShortcutEntry, os: Os, build: Build): readonly Chord[] {
  return entry.chords.filter(
    chord =>
      (chord.os === undefined || chord.os === os) &&
      (chord.where === undefined || build === 'desktop')
  )
}

/** Where a key is pressed: the platform, the build and the scope on screen besides `global`. */
export interface ShortcutContext {
  readonly os: Os
  readonly build: Build
  readonly scope: ShortcutScope
}

/**
 * The entry of `entries` that a press of `keys` runs in `context`, with the chord it matched: of
 * the global entries and those of the scope on screen, the scope's own wins over a global one
 * with the same chord.
 */
export function resolveChord(
  entries: readonly ShortcutEntry[],
  context: ShortcutContext,
  keys: string
): { readonly entry: ShortcutEntry; readonly chord: Chord } | null {
  const matches = entries.flatMap(entry =>
    entry.scope === 'global' || entry.scope === context.scope
      ? chordsIn(entry, context.os, context.build)
          .filter(chord => chord.keys === keys)
          .map(chord => ({ entry, chord }))
      : []
  )
  return matches.find(match => match.entry.scope !== 'global') ?? matches[0] ?? null
}
