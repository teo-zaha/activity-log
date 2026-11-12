import { beforeEach, describe, expect, it } from 'vitest'
import { persistedBooleanSignal, persistedSignal } from './persisted-signal'

type Mode = 'a' | 'b'

const parseMode = (stored: string): Mode | null =>
  stored === 'a' || stored === 'b' ? stored : null

describe('persistedSignal', () => {
  beforeEach(() => localStorage.clear())

  it('starts from the default when nothing is stored', () => {
    expect(persistedSignal<Mode>('mode', 'a', parseMode)()).toBe('a')
    expect(localStorage.getItem('mode')).toBeNull()
  })

  it('restores a stored value', () => {
    localStorage.setItem('mode', 'b')
    expect(persistedSignal<Mode>('mode', 'a', parseMode)()).toBe('b')
  })

  it('falls back to the default when the stored value does not parse', () => {
    localStorage.setItem('mode', 'garbage')
    expect(persistedSignal<Mode>('mode', 'a', parseMode)()).toBe('a')
  })

  it('writes through on set and update', () => {
    const mode = persistedSignal<Mode>('mode', 'a', parseMode)

    mode.set('b')
    expect(mode()).toBe('b')
    expect(localStorage.getItem('mode')).toBe('b')

    mode.update(current => (current === 'b' ? 'a' : 'b'))
    expect(mode()).toBe('a')
    expect(localStorage.getItem('mode')).toBe('a')
  })
})

describe('persistedBooleanSignal', () => {
  beforeEach(() => localStorage.clear())

  it('uses the default, then the stored value', () => {
    expect(persistedBooleanSignal('flag', true)()).toBe(true)
    localStorage.setItem('flag', 'false')
    expect(persistedBooleanSignal('flag', true)()).toBe(false)
  })

  it('writes through on set and update', () => {
    const flag = persistedBooleanSignal('flag')

    flag.set(true)
    expect(localStorage.getItem('flag')).toBe('true')

    flag.update(current => !current)
    expect(flag()).toBe(false)
    expect(localStorage.getItem('flag')).toBe('false')
  })
})
