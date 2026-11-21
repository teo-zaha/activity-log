import { describe, expect, it } from 'vitest'
import { ariaChord, chordOf, formatChord, menuKeyOf, osOf } from './key-chord'

const press = (key: string, init: KeyboardEventInit = {}): KeyboardEvent =>
  new KeyboardEvent('keydown', { key, ...init })

describe('chordOf', () => {
  it('reads ⌘ as mod on macOS and Ctrl as mod elsewhere', () => {
    expect(chordOf(press('r', { metaKey: true }), 'mac')).toBe('mod+r')
    expect(chordOf(press('r', { ctrlKey: true }), 'other')).toBe('mod+r')
  })

  it('names the other platform’s modifier as itself, so no chord matches it', () => {
    expect(chordOf(press('r', { ctrlKey: true }), 'mac')).toBe('ctrl+r')
    expect(chordOf(press('r', { metaKey: true }), 'other')).toBe('meta+r')
  })

  it('counts Shift on letters and named keys, not on the character it makes', () => {
    expect(chordOf(press('?', { shiftKey: true }), 'other')).toBe('?')
    expect(chordOf(press('R', { metaKey: true, shiftKey: true }), 'mac')).toBe('mod+shift+r')
    expect(chordOf(press('F5', { shiftKey: true }), 'other')).toBe('shift+f5')
  })

  it('reads named keys in lower case', () => {
    expect(chordOf(press('Escape'), 'mac')).toBe('escape')
    expect(chordOf(press('ArrowLeft', { altKey: true }), 'other')).toBe('alt+arrowleft')
    expect(chordOf(press('[', { metaKey: true }), 'mac')).toBe('mod+[')
  })

  it('has no chord for a modifier alone or a key being composed', () => {
    expect(chordOf(press('Meta', { metaKey: true }), 'mac')).toBeNull()
    expect(chordOf(press('Shift', { shiftKey: true }), 'other')).toBeNull()
    expect(chordOf(press('Dead'), 'mac')).toBeNull()
    expect(chordOf(press('Process'), 'other')).toBeNull()
  })
})

describe('formatChord and ariaChord', () => {
  it('write ⌘ on macOS and Ctrl elsewhere', () => {
    expect(formatChord('mod+r', 'mac')).toBe('⌘R')
    expect(formatChord('mod+r', 'other')).toBe('Ctrl+R')
    expect(formatChord('alt+arrowleft', 'other')).toBe('Alt+←')
    expect(formatChord('mod+[', 'mac')).toBe('⌘[')
    expect(formatChord('escape', 'mac')).toBe('Esc')
    expect(formatChord('/', 'other')).toBe('/')
  })

  it('name the keys as `aria-keyshortcuts` does', () => {
    expect(ariaChord('mod+r', 'mac')).toBe('Meta+R')
    expect(ariaChord('mod+f', 'other')).toBe('Control+F')
    expect(ariaChord('alt+arrowleft', 'other')).toBe('Alt+ArrowLeft')
    expect(ariaChord('escape', 'mac')).toBe('Escape')
    expect(ariaChord('f5', 'other')).toBe('F5')
  })
})

describe('osOf', () => {
  it('uses ⌘ on Apple’s systems', () => {
    expect(osOf('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15')).toBe('mac')
    expect(osOf('Mozilla/5.0 (iPad; CPU OS 18_0 like Mac OS X)')).toBe('mac')
    expect(osOf('Mozilla/5.0 (Windows NT 10.0; Win64; x64) Edg/131.0')).toBe('other')
    expect(osOf('Mozilla/5.0 (X11; Linux x86_64)')).toBe('other')
  })
})

describe('menuKeyOf', () => {
  it('gives a ⌘ chord the key of its menu accelerator, and no other chord one', () => {
    expect(menuKeyOf('mod+r')).toBe('r')
    expect(menuKeyOf('mod+[')).toBe('bracketLeft')
    expect(menuKeyOf('mod+,')).toBe('comma')
    expect(menuKeyOf('mod+3')).toBe('digit3')
    expect(menuKeyOf('alt+arrowleft')).toBeNull()
    expect(menuKeyOf('mod+escape')).toBeNull()
    expect(menuKeyOf('/')).toBeNull()
    expect(menuKeyOf('f5')).toBeNull()
  })
})
