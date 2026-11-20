import { vi } from 'vitest'

/** jsdom has no layout, so these observers never report anything. */
class InertObserver {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
  takeRecords(): [] {
    return []
  }
}

/**
 * Installs no-op `ResizeObserver` and `IntersectionObserver` globals for the current test file,
 * where they are missing (jsdom); a browser's layout specs keep the real ones.
 */
export function stubLayoutObservers(): void {
  if (typeof globalThis.ResizeObserver !== 'function')
    vi.stubGlobal('ResizeObserver', InertObserver)
  if (typeof globalThis.IntersectionObserver !== 'function') {
    vi.stubGlobal('IntersectionObserver', InertObserver)
  }
}
