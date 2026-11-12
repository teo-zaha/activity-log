import { BreakpointObserver } from '@angular/cdk/layout'
import { inject, type Signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { map } from 'rxjs'

/** Mirrors `$breakpoints` in `components/_dashboard-shared.scss`. */
export const BREAKPOINTS = {
  'desktop-compact': 1100,
  tablet: 900,
  'window-min': 820,
  phone: 640,
  'phone-narrow': 400,
} as const

export type BreakpointName = keyof typeof BREAKPOINTS

/** True while the viewport is at or below the named breakpoint (same as SCSS `ui.down`). */
export function injectBreakpointDown(name: BreakpointName): Signal<boolean> {
  const query = `(max-width: ${BREAKPOINTS[name]}px)`
  const observer = inject(BreakpointObserver)
  return toSignal(observer.observe(query).pipe(map(state => state.matches)), {
    initialValue: observer.isMatched(query),
  })
}
