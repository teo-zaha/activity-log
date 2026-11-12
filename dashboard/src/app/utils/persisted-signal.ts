import { linkedSignal, type WritableSignal } from '@angular/core'

function persisted<T>(key: string, initial: T, serialize: (value: T) => string): WritableSignal<T> {
  return linkedSignal(() => initial, {
    set: (value, rawSet) => {
      rawSet(value)
      localStorage.setItem(key, serialize(value))
    },
  })
}

export function persistedSignal<T extends string>(
  key: string,
  defaultValue: T,
  parse: (stored: string) => T | null = (stored: string): T => stored as T
): WritableSignal<T> {
  const stored = localStorage.getItem(key)
  const initial = stored !== null ? (parse(stored) ?? defaultValue) : defaultValue
  return persisted(key, initial, value => value)
}

export function persistedBooleanSignal(key: string, defaultValue = false): WritableSignal<boolean> {
  const stored = localStorage.getItem(key)
  const initial = stored !== null ? stored === 'true' : defaultValue
  return persisted(key, initial, String)
}
