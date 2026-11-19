import { vi } from 'vitest'

/** Layout observers for jsdom that keep count of the ones still observing. */
class CountingObserver {
  static readonly live = new Set<CountingObserver>()

  observe(): void {
    CountingObserver.live.add(this)
  }
  unobserve(): void {}
  disconnect(): void {
    CountingObserver.live.delete(this)
  }
  takeRecords(): [] {
    return []
  }
}

class CountingIntersectionObserver extends CountingObserver {}

/** Installs counting `ResizeObserver` and `IntersectionObserver` globals until `vi.unstubAllGlobals()`. */
export function stubCountingObservers(): void {
  CountingObserver.live.clear()
  vi.stubGlobal('ResizeObserver', CountingObserver)
  vi.stubGlobal('IntersectionObserver', CountingIntersectionObserver)
}

/** Intersection observers created and not yet disconnected. */
export function liveIntersectionObservers(): number {
  return [...CountingObserver.live].filter(
    observer => observer instanceof CountingIntersectionObserver
  ).length
}
