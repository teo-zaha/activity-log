import { downQuery } from '../utils/breakpoints'

/** Answers the phone breakpoint's media query with `matches`, which `set` changes. */
export function mockPhone(matches: boolean): { set(matches: boolean): void; restore(): void } {
  const list = Object.assign(new EventTarget(), { matches })
  const realMatchMedia = window.matchMedia
  window.matchMedia = (query: string): MediaQueryList =>
    query === downQuery('phone')
      ? {
          ...realMatchMedia(query),
          matches: list.matches,
          media: query,
          addEventListener: list.addEventListener.bind(list),
          removeEventListener: list.removeEventListener.bind(list),
        }
      : realMatchMedia(query)
  return {
    set(next: boolean): void {
      list.matches = next
      list.dispatchEvent(Object.assign(new Event('change'), { matches: next }))
    },
    restore(): void {
      window.matchMedia = realMatchMedia
    },
  }
}
