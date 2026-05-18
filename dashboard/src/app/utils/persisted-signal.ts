import { signal, type WritableSignal } from '@angular/core'

export function persistedSignal<T extends string>(
  key: string,
  defaultValue: T,
  parse: (stored: string) => T | null = (stored: string): T => stored as T
): WritableSignal<T> {
  const stored = localStorage.getItem(key)
  const initial = stored !== null ? (parse(stored) ?? defaultValue) : defaultValue

  const s = signal<T>(initial)
  const originalSet = s.set.bind(s)
  const originalUpdate = s.update.bind(s)

  s.set = (value: T): void => {
    originalSet(value)
    localStorage.setItem(key, value)
  }

  s.update = (fn: (value: T) => T): void => {
    originalUpdate((v: T): T => {
      const next = fn(v)
      localStorage.setItem(key, next)
      return next
    })
  }

  return s
}

export function persistedBooleanSignal(key: string, defaultValue = false): WritableSignal<boolean> {
  const stored = localStorage.getItem(key)
  const initial = stored !== null ? stored === 'true' : defaultValue

  const s = signal(initial)
  const originalSet = s.set.bind(s)
  const originalUpdate = s.update.bind(s)

  s.set = (value: boolean): void => {
    originalSet(value)
    localStorage.setItem(key, String(value))
  }

  s.update = (fn: (value: boolean) => boolean): void => {
    originalUpdate((v: boolean): boolean => {
      const next = fn(v)
      localStorage.setItem(key, String(next))
      return next
    })
  }

  return s
}
