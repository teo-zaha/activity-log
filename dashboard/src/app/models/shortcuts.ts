import type { MenuSubmenu } from '../generated/activity'
import type { ViewMode } from '../utils/app-url'
import {
  type CharacterChordKeys,
  type CommandChordKeys,
  type Os,
  isCharacterKeys,
} from '../utils/key-chord'
import { VIEW_LABELS } from './nav'

/**
 * Where an entry's keys work: `global` everywhere, or in the view whose route declares the
 * scope (`data.shortcutScope`), or on the page over the views that puts it on screen (the
 * details). A view the router keeps while another shows, or a page hides, holds no keys then
 * (`ShortcutService`).
 */
export const SHORTCUT_SCOPES = ['global', 'timeline', 'calendar', 'details'] as const
export type ShortcutScope = (typeof SHORTCUT_SCOPES)[number]

/** Where the dashboard runs: the desktop app, or a browser tab whose keys are the browser's. */
export type Build = 'desktop' | 'browser'

/**
 * Whether single-key shortcuts work (WCAG 2.1.4's way to turn them off): every chord that is a
 * character key alone, `/` and `?` included. Kept on the device (`ShortcutPreferences`).
 */
export type CharacterKeys = 'on' | 'off'

interface ChordPlace {
  /** Absent: every platform. */
  readonly os?: Os
  /** Absent: wherever the entry is. `desktop`: not in the browser, whose own key it is. */
  readonly where?: 'desktop'
}

/**
 * A character key alone (`R`, `/`): it types, so it never fires while typing, is never a menu
 * accelerator, and the single-key setting turns it off.
 */
export interface CharacterChord extends ChordPlace {
  readonly keys: CharacterChordKeys
}

/** A modified chord (⌘R) or a named key (Escape, F5). */
export interface CommandChord extends ChordPlace {
  readonly keys: CommandChordKeys
  /** Fires while focus is in an editable field. */
  readonly whileTyping: boolean
  /**
   * On macOS the menu's accelerator, once the menu is built from this table: only a modified
   * chord with no text-editing meaning there, since the menu takes the key before the page.
   */
  readonly menuAccelerator: boolean
}

/** A chord of an entry; its keys tell the two kinds apart. */
export type Chord = CharacterChord | CommandChord

export function isCharacterChord(chord: Chord): chord is CharacterChord {
  return isCharacterKeys(chord.keys)
}

/** Whether `chord` fires while focus is in a field: never for a character key. */
export function firesWhileTyping(chord: Chord): boolean {
  return isCharacterChord(chord) ? false : chord.whileTyping
}

/** A chord the macOS menu takes as its accelerator. */
export type MenuAcceleratorChord = CommandChord & { readonly menuAccelerator: true }

/**
 * Whether `chord` is the macOS menu's accelerator: never a character key. Narrows to the command
 * chords that are one, so a chord it refuses keeps its type.
 */
export function isMenuAccelerator(chord: Chord): chord is MenuAcceleratorChord {
  return !isCharacterChord(chord) && chord.menuAccelerator
}

export interface ShortcutEntry {
  readonly id: ShortcutId
  /** Verbatim in the help, beside a key tag and in the macOS menu. */
  readonly label: string
  readonly scope: ShortcutScope
  readonly chords: readonly Chord[]
  /** The macOS menu's submenu it is listed in; the menu is built from this table. */
  readonly menu?: MenuSubmenu
  /**
   * The global entry this scope's entry is on its own view: the same command, so it may share
   * that entry's chords, which it wins on its view, and the help shows them as one row. Absent for
   * a scope's own command (a future Work items `/`, which filters something else).
   */
  readonly standsFor?: ShortcutId
}

/** Every shortcut, in the help's order. */
export const SHORTCUT_IDS = [
  'app.collect',
  'app.filter',
  'timeline.filter',
  'view.list',
  'view.day',
  'view.workweek',
  'view.week',
  'app.settings',
  'app.export-markdown',
  'app.export-json',
  'app.back',
  'layer.close',
  'app.help',
  'calendar.previous',
  'calendar.next',
  'calendar.today',
  'details.previous',
  'details.next',
] as const

export type ShortcutId = (typeof SHORTCUT_IDS)[number]

/** The entry that shows each view; the macOS View menu lists them as the navigation groups them. */
export const VIEW_SHORTCUTS = {
  list: 'view.list',
  day: 'view.day',
  workweek: 'view.workweek',
  week: 'view.week',
} as const satisfies Readonly<Record<ViewMode, ShortcutId>>

/** A view's entry: ⌘1 to ⌘4 (Ctrl elsewhere) in the desktop app, and the digit alone. */
function viewEntry(
  view: ViewMode,
  keys: CommandChordKeys,
  digit: CharacterChordKeys
): Omit<ShortcutEntry, 'id'> {
  return {
    label: VIEW_LABELS[view],
    scope: 'global',
    menu: 'view',
    chords: [{ keys, where: 'desktop', whileTyping: true, menuAccelerator: true }, { keys: digit }],
  }
}

/*
 * A command's modified chords come before its character key, so the key tag shows ⌘R in the
 * desktop app and R in the browser. The character keys are the desktop chord without ⌘, and work
 * in every build.
 */
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
      { keys: 'r' },
    ],
  },
  // A browser keeps its own find; `/` is the web's filter key.
  'app.filter': {
    label: 'Filter Activity…',
    scope: 'global',
    menu: 'edit',
    chords: [
      { keys: 'mod+f', where: 'desktop', whileTyping: true, menuAccelerator: true },
      { keys: '/' },
    ],
  },
  'timeline.filter': {
    label: 'Filter activity',
    scope: 'timeline',
    standsFor: 'app.filter',
    chords: [{ keys: '/' }],
  },
  // ⌘1 to ⌘4 switch a browser's tabs.
  'view.list': viewEntry('list', 'mod+1', '1'),
  'view.day': viewEntry('day', 'mod+2', '2'),
  'view.workweek': viewEntry('workweek', 'mod+3', '3'),
  'view.week': viewEntry('week', 'mod+4', '4'),
  // ⌘, opens a browser's own settings.
  'app.settings': {
    label: 'Settings…',
    scope: 'global',
    menu: 'app',
    chords: [
      { keys: 'mod+,', where: 'desktop', whileTyping: true, menuAccelerator: true },
      { keys: ',' },
    ],
  },
  // Menu items without a key: the header's buttons do the same.
  'app.export-markdown': { label: 'Export Markdown…', scope: 'global', menu: 'file', chords: [] },
  'app.export-json': { label: 'Export JSON…', scope: 'global', menu: 'file', chords: [] },
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
    chords: [{ keys: '?' }],
  },
  // The Day, Work week and Week views' buttons: a day in Day, a week in the others.
  'calendar.previous': {
    label: 'Previous day or week',
    scope: 'calendar',
    chords: [{ keys: 'p' }],
  },
  'calendar.next': { label: 'Next day or week', scope: 'calendar', chords: [{ keys: 'n' }] },
  'calendar.today': { label: 'Today', scope: 'calendar', chords: [{ keys: 't' }] },
  // The stepper through a group of overlapping events.
  'details.previous': { label: 'Previous event', scope: 'details', chords: [{ keys: 'p' }] },
  'details.next': { label: 'Next event', scope: 'details', chords: [{ keys: 'n' }] },
}

export const SHORTCUTS: readonly ShortcutEntry[] = SHORTCUT_IDS.map(id => shortcut(id))

export function shortcut(id: ShortcutId): ShortcutEntry {
  return { id, ...ENTRIES[id] }
}

/** Which chords work: the platform, the build and whether single keys are on. */
export interface ChordsHere {
  readonly os: Os
  readonly build: Build
  readonly characterKeys: CharacterKeys
}

/**
 * The chords of `entry` that work `here`, first the one a key tag shows: of its platform, of its
 * build, and character keys only while single-key shortcuts are on.
 */
export function chordsIn(entry: ShortcutEntry, here: ChordsHere): readonly Chord[] {
  return entry.chords.filter(
    chord =>
      (chord.os === undefined || chord.os === here.os) &&
      (chord.where === undefined || here.build === 'desktop') &&
      (here.characterKeys === 'on' || !isCharacterChord(chord))
  )
}

/** Where a key is pressed: which chords work, and the scope on screen besides `global`. */
export interface ShortcutContext extends ChordsHere {
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
      ? chordsIn(entry, context)
          .filter(chord => chord.keys === keys)
          .map(chord => ({ entry, chord }))
      : []
  )
  return matches.find(match => match.entry.scope !== 'global') ?? matches[0] ?? null
}
