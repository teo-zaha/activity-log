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

/** Installs no-op `ResizeObserver` and `IntersectionObserver` globals for the current test file. */
export function stubLayoutObservers(): void {
  vi.stubGlobal('ResizeObserver', InertObserver)
  vi.stubGlobal('IntersectionObserver', InertObserver)
}
