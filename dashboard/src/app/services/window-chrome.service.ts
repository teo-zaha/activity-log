import { Service, inject, signal } from '@angular/core'
import { listener, setupSync } from '@signality/core/browser/listener'
import type { Window as TauriWindow } from '@tauri-apps/api/window'
import {
  APP_CONTEXT_MENU,
  type SelectionRects,
  keepsNativeMenu,
  selectionRects,
} from '../utils/context-menu'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'

/** The pointer over the maximize button, as `desktop/src/window_chrome/` reports it. */
export interface MaximizeButtonState {
  hovered: boolean
  pressed: boolean
}

/**
 * The desktop window's frame: dragging and maximizing it from the app's own surfaces and, where
 * the window has no title bar (Windows, `tauri.windows.conf.json`), its buttons. In the desktop
 * app the web view's own right-click menu shows only in text fields and over selected text.
 */
@Service()
export class WindowChromeService {
  private readonly runtime = inject(RuntimeEnvironmentService)

  /** The app draws the window buttons and the header is the title bar; set in `index.html`. */
  readonly frameless =
    typeof document !== 'undefined' && document.documentElement.classList.contains('frameless')
  readonly maximized = signal(false)
  /**
   * On Windows 11 a native window over the maximize button shows the snap layouts, so the page
   * never sees the pointer there; the desktop app reports it instead.
   */
  readonly maximizeButton = signal<MaximizeButtonState>({ hovered: false, pressed: false })

  constructor() {
    if (this.runtime.isTauri()) this.limitContextMenu()
    if (this.frameless) void this.followWindow()
  }

  /**
   * The selection is read when the button goes down, before WebKit selects the word under a
   * right-click; the menu key has no press and reads it as it is. Registered at once, so the
   * splash's own blocker in `index.html` can go.
   */
  private limitContextMenu(): void {
    let pressed: SelectionRects | null = null
    setupSync(() => {
      listener.capture(document, 'pointerdown', () => {
        pressed = selectionRects(document.getSelection())
      })
      listener(document, 'contextmenu', event => {
        const selected =
          event.button < 0 || !pressed ? selectionRects(document.getSelection()) : pressed
        pressed = null
        if (!keepsNativeMenu(event, selected)) event.preventDefault()
      })
    })
    document.dispatchEvent(new Event(APP_CONTEXT_MENU))
  }

  /** Puts the snap layouts over `button`, which keeps its place at the window's top right. */
  async attachMaximizeButton(button: HTMLElement): Promise<void> {
    if (!this.runtime.isTauri()) return
    const { top, right, width, height } = button.getBoundingClientRect()
    const { invoke } = await import('@tauri-apps/api/core')
    // Without it the button still maximizes, only without the snap layouts.
    await invoke('attach_maximize_button', {
      top,
      right: window.innerWidth - right,
      width,
      height,
    }).catch(() => undefined)
  }

  /** Moves the window with the pointer, as a title bar does; Windows snaps it at the edges. */
  async startDrag(event: MouseEvent): Promise<void> {
    if (!this.runtime.isTauri() || event.button !== 0) return
    event.preventDefault()
    await (await this.window()).startDragging()
  }

  async toggleMaximize(event?: MouseEvent): Promise<void> {
    if (!this.runtime.isTauri() || (event && event.button !== 0)) return
    event?.preventDefault()
    await (await this.window()).toggleMaximize()
  }

  async minimize(): Promise<void> {
    if (this.runtime.isTauri()) await (await this.window()).minimize()
  }

  async close(): Promise<void> {
    if (this.runtime.isTauri()) await (await this.window()).close()
  }

  private async window(): Promise<TauriWindow> {
    const { getCurrentWindow } = await import('@tauri-apps/api/window')
    return getCurrentWindow()
  }

  /**
   * Maximize shows as restore while the window is maximized. Hand-written: Tauri's window events
   * are not DOM events, which Signality covers. The service lives as long as the window.
   */
  private async followWindow(): Promise<void> {
    const appWindow = await this.window()
    const sync = async (): Promise<void> => this.maximized.set(await appWindow.isMaximized())
    // Listening first, so a change between the first read and the listener is not missed.
    await appWindow.onResized(() => void sync())
    await appWindow.listen<MaximizeButtonState>('maximize-button', ({ payload }) =>
      this.maximizeButton.set(payload)
    )
    await sync()
  }
}
