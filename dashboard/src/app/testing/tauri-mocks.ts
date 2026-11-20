import { clearMocks } from '@tauri-apps/api/mocks'

/**
 * Undoes `mockIPC`/`mockWindows` completely. `clearMocks` leaves an empty
 * `window.__TAURI_INTERNALS__` behind, which makes a later spec in the same worker take the page
 * for the desktop app (`RuntimeEnvironmentService.isTauri`).
 */
export function clearTauriMocks(): void {
  clearMocks()
  Reflect.deleteProperty(window, '__TAURI_INTERNALS__')
}
