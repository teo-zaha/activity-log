/**
 * The same as `$breakpoints` in `components/_dashboard-shared.scss`, which
 * `scripts/breakpoints.test.mjs` checks.
 */
export const BREAKPOINTS = {
  'desktop-compact': 1100,
  tablet: 900,
  'window-min': 820,
  phone: 640,
  'phone-narrow': 400,
} as const

export type BreakpointName = keyof typeof BREAKPOINTS

/** The media query for at or below the named breakpoint, as SCSS `ui.down` writes it. */
export function downQuery(name: BreakpointName): string {
  return `(max-width: ${BREAKPOINTS[name]}px)`
}
