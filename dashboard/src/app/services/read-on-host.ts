import { untracked, type Signal } from '@angular/core'

/**
 * Runs `read` until it is answered, or fails, under the host it was sent under: one that comes
 * back or fails after another host was installed (the host restarted meanwhile) is sent again,
 * under the host on screen. So what it resolves or rejects with never comes from a host that is
 * gone. `hostEpoch` is `ActivityDataService.hostEpoch`.
 */
export async function readOnHost<T>(
  hostEpoch: Signal<string | null>,
  read: () => Promise<T>
): Promise<T> {
  for (;;) {
    const host = untracked(hostEpoch)
    let outcome:
      | { readonly answered: true; readonly value: T }
      | { readonly answered: false; readonly error: unknown }
    try {
      outcome = { answered: true, value: await read() }
    } catch (error) {
      outcome = { answered: false, error }
    }
    if (untracked(hostEpoch) !== host) continue
    if (outcome.answered) return outcome.value
    throw outcome.error
  }
}
