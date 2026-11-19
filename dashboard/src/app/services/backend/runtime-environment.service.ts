import { Service } from '@angular/core'

@Service()
export class RuntimeEnvironmentService {
  private readonly tauri =
    typeof window !== 'undefined' && Object.hasOwn(window, '__TAURI_INTERNALS__')

  constructor() {
    if (this.tauri) {
      void this.syncFullscreenClass()
    }
  }

  isTauri(): boolean {
    return this.tauri
  }

  private async syncFullscreenClass(): Promise<void> {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    const appWindow = getCurrentWindow()

    // Reads can overlap and answer out of order; only the latest one counts.
    let latest = 0
    const applyFullscreenClass = async (): Promise<void> => {
      const read = ++latest
      const fullscreen = await appWindow.isFullscreen()
      if (read === latest) document.documentElement.classList.toggle('fullscreen', fullscreen)
    }

    // Listening first: the desktop app restores fullscreen just after the window shows, maybe
    // between the first read and the listener.
    await appWindow.onResized(() => {
      void applyFullscreenClass()
    })
    await applyFullscreenClass()
  }
}
