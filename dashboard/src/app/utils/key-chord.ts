/**
 * Key chords for the app's shortcuts (`models/shortcuts.ts`): reading one from a key press,
 * and writing one for people (the key tags, the help) and for `aria-keyshortcuts`. `mod` is ⌘ on
 * macOS and Ctrl elsewhere.
 */

/** Which keys the platform's shortcuts use: ⌘ on macOS (the browser there too), Ctrl elsewhere. */
export type Os = 'mac' | 'other'

/** The keys the registry names, as `KeyboardEvent.key` reads them, in lower case. */
export type ChordKey = 'escape' | 'f5' | 'arrowleft' | '/' | '?' | '[' | 'f' | 'r'

/** A chord the registry names: at most one modifier, then the key. */
export type ChordKeys = ChordKey | `${'mod' | 'alt'}+${ChordKey}`

const MODIFIER_KEYS = new Set(['Alt', 'AltGraph', 'Control', 'Meta', 'Shift', 'OS'])

/** `KeyboardEvent.key` values that name no key a chord can hold. */
const NO_KEY = new Set(['Dead', 'Process', 'Unidentified'])

/** The platform as the browser names it: Apple's systems use ⌘. */
export function osOf(userAgent: string): Os {
  return /Mac|iPhone|iPad|iPod/.test(userAgent) ? 'mac' : 'other'
}

/**
 * The chord a key press is, written as the registry writes chords: modifiers in a fixed order,
 * then the key. Shift counts only where it does not make the character (`?` is Shift+/ on many
 * layouts, and the registry names `?`); Ctrl on macOS and the Windows key elsewhere are named as
 * themselves, so no chord matches them. Null for a modifier alone or a key still being composed.
 */
export function chordOf(event: KeyboardEvent, os: Os): string | null {
  const key = event.key
  if (key === '' || MODIFIER_KEYS.has(key) || NO_KEY.has(key)) return null
  const character = key.length === 1
  const letter = character && key.toLowerCase() !== key.toUpperCase()
  const mod = os === 'mac' ? event.metaKey : event.ctrlKey
  const unused = os === 'mac' ? event.ctrlKey : event.metaKey
  const parts = [
    mod ? 'mod' : null,
    unused ? (os === 'mac' ? 'ctrl' : 'meta') : null,
    event.altKey ? 'alt' : null,
    event.shiftKey && (!character || letter) ? 'shift' : null,
    key.toLowerCase(),
  ]
  return parts.filter(part => part !== null).join('+')
}

function isChordKey(value: string): value is ChordKey {
  return Object.hasOwn(KEY_LABELS, value)
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
  f: 'F',
  r: 'R',
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
  f: 'F',
  r: 'R',
}

/** A chord as `aria-keyshortcuts` writes it: `Meta+R` on macOS, `Control+R` elsewhere. */
export function ariaChord(keys: ChordKeys, os: Os): string {
  const { modifier, key } = split(keys)
  const name = ARIA_KEYS[key]
  if (modifier === null) return name
  const prefix = modifier === 'alt' ? 'Alt' : os === 'mac' ? 'Meta' : 'Control'
  return `${prefix}+${name}`
}
