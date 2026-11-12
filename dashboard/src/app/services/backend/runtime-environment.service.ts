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

    const applyFullscreenClass = async (): Promise<void> => {
      document.documentElement.classList.toggle('fullscreen', await appWindow.isFullscreen())
    }

    await applyFullscreenClass()
    await appWindow.onResized(() => {
      void applyFullscreenClass()
    })
  }
}
