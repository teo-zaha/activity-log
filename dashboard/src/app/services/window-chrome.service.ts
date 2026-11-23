import { DestroyRef, Service, inject, signal } from '@angular/core'
import { listener, setupSync } from '@signality/core/browser/listener'
import type { Window as TauriWindow } from '@tauri-apps/api/window'
import { DESKTOP_COMMANDS, DESKTOP_EVENTS, type MaximizeButtonState } from '../generated/activity'
import {
  APP_CONTEXT_MENU,
  type SelectionRects,
  keepsNativeMenu,
  selectionRects,
} from '../utils/context-menu'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'
import { failureWhile } from './failure'

/**
 * The desktop window's frame: dragging and maximizing it from the app's own surfaces and, where
 * the window has no title bar (Windows, `tauri.windows.conf.json`), its buttons; the root's
 * `fullscreen` class while the window is fullscreen. In the desktop app the web view's own
 * right-click menu shows only in text fields and over selected text.
 */
@Service()
export class WindowChromeService {
  private readonly runtime = inject(RuntimeEnvironmentService)
  private destroyed = false
  /** Stops each of the window's listeners. */
  private readonly unlisten: (() => void)[] = []

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
    if (!this.runtime.isTauri()) return
    inject(DestroyRef).onDestroy(() => {
      this.destroyed = true
      this.unlisten.forEach(stop => stop())
    })
    this.limitContextMenu()
    void this.followWindow()
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
    await invoke(DESKTOP_COMMANDS.attachMaximizeButton, {
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
   * One listener for the window's resizes: the root's `fullscreen` class (the header's title bar
   * moves the window only outside fullscreen) and, where the app draws the window buttons,
   * maximize shown as restore while the window is maximized. Hand-written: Tauri's window events
   * are not DOM events, which Signality covers. The service lives as long as the window; once
   * destroyed (a spec's injector), it stops listening.
   */
  private async followWindow(): Promise<void> {
    const appWindow = await this.window()
    if (this.destroyed) return
    const root = document.documentElement
    // Reads can overlap and answer out of order; only the latest one counts.
    let latest = 0
    let warned = false
    const sync = async (): Promise<void> => {
      const read = ++latest
      // Each read that answers applies; one the window refuses leaves its own state as it was.
      const [fullscreen, maximized] = await Promise.allSettled([
        appWindow.isFullscreen(),
        this.frameless ? appWindow.isMaximized() : Promise.resolve(false),
      ])
      if (read !== latest || this.destroyed) return
      if (fullscreen.status === 'fulfilled') root.classList.toggle('fullscreen', fullscreen.value)
      if (maximized.status === 'fulfilled') this.maximized.set(maximized.value)
      const refused = [fullscreen, maximized].find(result => result.status === 'rejected')
      if (refused && !warned) {
        // Once: every resize would read it again.
        warned = true
        const failed = failureWhile('Reading the window’s state', refused.reason)
        console.warn(failed.message, failed.cause)
      }
    }
    // Listening first: the desktop app restores fullscreen just after the window shows, maybe
    // between the first read and the listener.
    this.keep(await appWindow.onResized(() => void sync()))
    if (this.frameless) {
      this.keep(
        await appWindow.listen<MaximizeButtonState>(DESKTOP_EVENTS.maximizeButton, ({ payload }) =>
          this.maximizeButton.set(payload)
        )
      )
    }
    if (!this.destroyed) await sync()
  }

  /** Keeps a listener until the service is destroyed; one that came in after is stopped. */
  private keep(stop: () => void): void {
    if (this.destroyed) stop()
    else this.unlisten.push(stop)
  }
}
