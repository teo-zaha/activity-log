import { Service } from '@angular/core'

/** Whether the dashboard runs in the desktop app's web view or in a browser. */
@Service()
export class RuntimeEnvironmentService {
  private readonly tauri =
    typeof window !== 'undefined' && Object.hasOwn(window, '__TAURI_INTERNALS__')

  isTauri(): boolean {
    return this.tauri
  }
}
