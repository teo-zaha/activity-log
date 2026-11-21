import { BREAKPOINTS, downQuery, type BreakpointName } from '../utils/breakpoints'

/** Media queries of named breakpoints answered as a spec says, which `set` changes. */
export interface MockBreakpoints<Name extends BreakpointName> {
  set(name: Name, matches: boolean): void
  restore(): void
}

/**
 * Answers the media query of each breakpoint named in `matches` (`downQuery(name)`) with its value;
 * any other query gets the real answer, none matching in jsdom.
 */
export function mockBreakpoints<Name extends BreakpointName>(
  matches: Readonly<Record<Name, boolean>>
): MockBreakpoints<Name> {
  const lists = new Map<string, EventTarget & { matches: boolean }>()
  for (const [name, value] of Object.entries(matches)) {
    if (!isBreakpointName(name) || typeof value !== 'boolean') continue
    lists.set(downQuery(name), Object.assign(new EventTarget(), { matches: value }))
  }
  const realMatchMedia = window.matchMedia
  window.matchMedia = (query: string): MediaQueryList => {
    const list = lists.get(query)
    return list
      ? {
          ...realMatchMedia(query),
          matches: list.matches,
          media: query,
          addEventListener: list.addEventListener.bind(list),
          removeEventListener: list.removeEventListener.bind(list),
        }
      : realMatchMedia(query)
  }
  return {
    set(name: Name, next: boolean): void {
      const list = lists.get(downQuery(name))
      if (list === undefined) throw new Error(`Breakpoint ${name} is not mocked`)
      list.matches = next
      list.dispatchEvent(Object.assign(new Event('change'), { matches: next }))
    },
    restore(): void {
      window.matchMedia = realMatchMedia
    },
  }
}

/** Answers the phone breakpoint's media query with `matches`, which `set` changes. */
export function mockPhone(matches: boolean): { set(matches: boolean): void; restore(): void } {
  const media = mockBreakpoints({ phone: matches })
  return { set: next => media.set('phone', next), restore: () => media.restore() }
}

function isBreakpointName(name: string): name is BreakpointName {
  return Object.hasOwn(BREAKPOINTS, name)
}
