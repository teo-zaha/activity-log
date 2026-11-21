/**
 * Key chords for the app's shortcuts (`models/shortcuts.ts`): reading one from a key press,
 * and writing one for people (the key tags, the help) and for `aria-keyshortcuts`. `mod` is ⌘ on
 * macOS and Ctrl elsewhere.
 */

import type { MenuKey } from '../generated/activity'

/** Which keys the platform's shortcuts use: ⌘ on macOS (the browser there too), Ctrl elsewhere. */
export type Os = 'mac' | 'other'

/**
 * A key that types a character, WCAG 2.1.4's "letter, punctuation, number or symbol", as the
 * character typed (a letter outside A–Z as its physical key, `chordOf`).
 */
export const CHARACTER_KEYS = [
  '/',
  '?',
  '[',
  ',',
  '1',
  '2',
  '3',
  '4',
  'f',
  'n',
  'p',
  'r',
  't',
] as const
export type CharacterKey = (typeof CHARACTER_KEYS)[number]

/** A key that types nothing, as `KeyboardEvent.key` names it, in lower case. */
export type NamedKey = 'escape' | 'f5' | 'arrowleft'

/** The keys the registry names. */
export type ChordKey = CharacterKey | NamedKey

/**
 * A character key alone: it types, so it never fires while typing, is never a menu accelerator,
 * and the single-key setting turns it off.
 */
export type CharacterChordKeys = CharacterKey

/** A modifier and a key, or a named key alone. */
export type CommandChordKeys = NamedKey | `${'mod' | 'alt'}+${ChordKey}`

/** A chord the registry names: at most one modifier, then the key. */
export type ChordKeys = CharacterChordKeys | CommandChordKeys

const MODIFIER_KEYS = new Set(['Alt', 'AltGraph', 'Control', 'Meta', 'Shift', 'OS'])

/** `KeyboardEvent.key` values that name no key a chord can hold. */
const NO_KEY = new Set(['Dead', 'Process', 'Unidentified'])

/** The platform as the browser names it: Apple's systems use ⌘. */
export function osOf(userAgent: string): Os {
  return /Mac|iPhone|iPad|iPod/.test(userAgent) ? 'mac' : 'other'
}

/** The Latin letter of a letter key's physical place (`KeyR`), whatever the layout types there. */
const PHYSICAL_LETTER = /^Key([A-Z])$/

const LATIN_LETTER = /^[a-z]$/i

/**
 * The chord a key press is, written as the registry writes chords: modifiers in a fixed order,
 * then the key, as the character the layout types: `R` is the key printed R on QWERTY, Dvorak or
 * Colemak alike, as the macOS menu reads ⌘R, and `/` is Shift+7 on a German layout. Only a letter
 * outside A–Z (Cyrillic, Greek, Hebrew) is read by its physical key, so `R` and ⌘R are where a
 * Latin keyboard has them there. Shift counts only where it does not make the character (`?` is
 * Shift+/ on many layouts, and the registry names `?`); Ctrl on macOS and the Windows key
 * elsewhere are named as themselves, so no chord matches them. Null for a modifier alone or a key
 * still being composed.
 */
export function chordOf(event: KeyboardEvent, os: Os): string | null {
  const key = event.key
  if (key === '' || MODIFIER_KEYS.has(key) || NO_KEY.has(key)) return null
  const character = key.length === 1
  const letter = character && key.toLowerCase() !== key.toUpperCase()
  const physical =
    letter && !LATIN_LETTER.test(key) ? PHYSICAL_LETTER.exec(event.code)?.[1] : undefined
  const mod = os === 'mac' ? event.metaKey : event.ctrlKey
  const unused = os === 'mac' ? event.ctrlKey : event.metaKey
  const parts = [
    mod ? 'mod' : null,
    unused ? (os === 'mac' ? 'ctrl' : 'meta') : null,
    event.altKey ? 'alt' : null,
    event.shiftKey && (!character || letter) ? 'shift' : null,
    (physical ?? key).toLowerCase(),
  ]
  return parts.filter(part => part !== null).join('+')
}

function isChordKey(value: string): value is ChordKey {
  return Object.hasOwn(KEY_LABELS, value)
}

/** Whether `keys` is a character key alone, the chord kind that types. */
export function isCharacterKeys(keys: ChordKeys): keys is CharacterChordKeys {
  return CHARACTER_KEYS.some(key => key === keys)
}

/** A chord's modifier and key; the registry's keys hold no `+`, so the first one splits. */
function split(keys: ChordKeys): { modifier: 'mod' | 'alt' | null; key: ChordKey } {
  if (isChordKey(keys)) return { modifier: null, key: keys }
  const [modifier, key] = keys.split('+')
  if (key === undefined || !isChordKey(key)) throw new Error(`Not a chord: ${keys}`)
  return { modifier: modifier === 'mod' ? 'mod' : 'alt', key }
}

const KEY_LABELS: Readonly<Record<ChordKey, string>> = {
  escape: 'Esc',
  f5: 'F5',
  arrowleft: '←',
  '/': '/',
  '?': '?',
  '[': '[',
  ',': ',',
  '1': '1',
  '2': '2',
  '3': '3',
  '4': '4',
  f: 'F',
  n: 'N',
  p: 'P',
  r: 'R',
  t: 'T',
}

/** How a chord reads on a key tag and in the help: `⌘R` on macOS, `Ctrl+R` elsewhere. */
export function formatChord(keys: ChordKeys, os: Os): string {
  const { modifier, key } = split(keys)
  const label = KEY_LABELS[key]
  if (modifier === null) return label
  if (os === 'mac') return `${modifier === 'mod' ? '⌘' : '⌥'}${label}`
  return `${modifier === 'mod' ? 'Ctrl' : 'Alt'}+${label}`
}

const ARIA_KEYS: Readonly<Record<ChordKey, string>> = {
  escape: 'Escape',
  f5: 'F5',
  arrowleft: 'ArrowLeft',
  '/': '/',
  '?': '?',
  '[': '[',
  ',': ',',
  '1': '1',
  '2': '2',
  '3': '3',
  '4': '4',
  f: 'F',
  n: 'N',
  p: 'P',
  r: 'R',
  t: 'T',
}

/** A chord as `aria-keyshortcuts` writes it: `Meta+R` on macOS, `Control+R` elsewhere. */
export function ariaChord(keys: ChordKeys, os: Os): string {
  const { modifier, key } = split(keys)
  const name = ARIA_KEYS[key]
  if (modifier === null) return name
  const prefix = modifier === 'alt' ? 'Alt' : os === 'mac' ? 'Meta' : 'Control'
  return `${prefix}+${name}`
}

/** The key a menu accelerator of each key is, where the menu can have one (with ⌘). */
const MENU_KEYS: Readonly<Record<ChordKey, MenuKey | null>> = {
  escape: null,
  f5: null,
  arrowleft: null,
  '/': null,
  '?': null,
  '[': 'bracketLeft',
  ',': 'comma',
  '1': 'digit1',
  '2': 'digit2',
  '3': 'digit3',
  '4': 'digit4',
  f: 'f',
  n: null,
  p: null,
  r: 'r',
  t: null,
}

/**
 * The native menu's accelerator for a chord, ⌘ and its key: only a `mod` chord has one, and only
 * of a key the menu knows.
 */
export function menuKeyOf(keys: ChordKeys): MenuKey | null {
  const { modifier, key } = split(keys)
  return modifier === 'mod' ? MENU_KEYS[key] : null
}
