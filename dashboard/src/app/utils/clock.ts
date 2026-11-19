import { DestroyRef, inject, signal, type Signal } from '@angular/core'

const MINUTE = 60_000

// Hand-written: Signality's `interval` cannot align its ticks to the start of a minute.

/**
 * The current time in epoch milliseconds, updated at the start of every minute by one timer
 * that stops when the injecting component is destroyed.
 */
export function injectMinuteClock(): Signal<number> {
  const now = signal(Date.now())
  let timer: ReturnType<typeof setTimeout>
  const schedule = (): void => {
    timer = setTimeout(
      () => {
        now.set(Date.now())
        schedule()
      },
      MINUTE - (Date.now() % MINUTE)
    )
  }
  schedule()
  inject(DestroyRef).onDestroy(() => clearTimeout(timer))
  return now.asReadonly()
}
