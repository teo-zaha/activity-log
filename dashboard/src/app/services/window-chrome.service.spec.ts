import { TestBed } from '@angular/core/testing'
import { emit } from '@tauri-apps/api/event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from 'vitest'
import { clearTauriMocks } from '../testing/tauri-mocks'
import { APP_CONTEXT_MENU } from '../utils/context-menu'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'
import { WindowChromeService } from './window-chrome.service'

/** Where the selected text lies on screen; jsdom lays nothing out. */
let selectedRects: DOMRect[] = []

function startChrome(tauri: boolean): void {
  // A window that is never fullscreen nor maximized, for what the service reads of it.
  if (tauri && !Object.hasOwn(window, '__TAURI_INTERNALS__')) {
    mockWindows('main')
    mockIPC(cmd => (cmd.startsWith('plugin:window|is_') ? false : 0))
  }
  TestBed.configureTestingModule({
    providers: [
      { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => tauri } },
    ],
  })
  TestBed.inject(WindowChromeService)
}

/** Shows `element` in the page. */
function shown<T extends Element>(element: T): T {
  document.body.append(element)
  return element
}

/** Selects `element`'s text, drawn as `rects`; with no element, nothing is selected. */
function select(element?: Element, ...rects: DOMRect[]): void {
  const selection = document.getSelection()
  if (element) selection?.selectAllChildren(element)
  else selection?.removeAllRanges()
  selectedRects = rects
}

/** True when the web view's own menu would show. */
function contextMenu(element: Element, init: MouseEventInit = {}): boolean {
  return element.dispatchEvent(
    new MouseEvent('contextmenu', { bubbles: true, cancelable: true, button: 2, ...init })
  )
}

function press(element: Element, init: MouseEventInit): void {
  element.dispatchEvent(new MouseEvent('pointerdown', { bubbles: true, button: 2, ...init }))
}

function paragraph(text: string): HTMLParagraphElement {
  const element = document.createElement('p')
  element.textContent = text
  return shown(element)
}

describe('WindowChromeService', () => {
  beforeAll(() => {
    Object.defineProperty(Range.prototype, 'getClientRects', {
      configurable: true,
      value: () => selectedRects,
    })
  })
  afterAll(() => Reflect.deleteProperty(Range.prototype, 'getClientRects'))
  afterEach(() => {
    select()
    document.body.replaceChildren()
    // The service stops listening as it is destroyed, through the mocks.
    TestBed.resetTestingModule()
    clearTauriMocks()
  })

  it('hides the right-click menu in the desktop app except in text fields', () => {
    startChrome(true)

    expect(contextMenu(shown(document.createElement('li')))).toBe(false)
    expect(contextMenu(shown(document.createElement('button')))).toBe(false)
    expect(contextMenu(shown(document.createElement('textarea')))).toBe(true)
    expect(contextMenu(shown(document.createElement('input')))).toBe(true)
  })

  it('judges a right-click by the selection from before the press', () => {
    startChrome(true)
    const text = paragraph('alpha bravo')
    const alpha = new DOMRect(0, 0, 40, 16)
    const bravo = new DOMRect(50, 0, 40, 16)
    const onBravo = { clientX: 60, clientY: 8 }

    // WebKit selects the word under a right-click between the press and `contextmenu`.
    press(text, onBravo)
    select(text, bravo)
    expect(contextMenu(text, onBravo)).toBe(false)

    select(text, alpha)
    press(text, onBravo)
    select(text, bravo)
    expect(contextMenu(text, onBravo)).toBe(false)

    press(text, onBravo)
    expect(contextMenu(text, onBravo)).toBe(true)
  })

  it('judges the menu key by the selection as it is', () => {
    startChrome(true)
    const text = paragraph('alpha')
    const row = shown(document.createElement('li'))

    press(text, { clientX: 300, clientY: 300 })
    select(text, new DOMRect(0, 0, 40, 16))
    expect(contextMenu(row, { button: -1 })).toBe(true)
    select()
    expect(contextMenu(row, { button: -1 })).toBe(false)
  })

  it('tells the splash in index.html to drop its own blocker', () => {
    let released = false
    document.addEventListener(APP_CONTEXT_MENU, () => (released = true), { once: true })

    startChrome(true)
    expect(released).toBe(true)
  })

  it('leaves the browser its own right-click menu', () => {
    let released = false
    const listen = (): boolean => (released = true)
    document.addEventListener(APP_CONTEXT_MENU, listen)

    startChrome(false)
    document.removeEventListener(APP_CONTEXT_MENU, listen)
    expect(released).toBe(false)
    expect(contextMenu(shown(document.createElement('li')))).toBe(true)
  })
})

describe('WindowChromeService in the desktop window', () => {
  const root = document.documentElement
  afterEach(() => {
    TestBed.resetTestingModule()
    clearTauriMocks()
    root.classList.remove('fullscreen', 'frameless')
  })

  /** Starts the window's services as the app does, the real runtime check included. */
  function startWindow(): void {
    TestBed.inject(RuntimeEnvironmentService)
    TestBed.inject(WindowChromeService)
  }

  it('listens to the window’s resizes once, also where it draws the window buttons', async () => {
    root.classList.add('frameless')
    const resizeListeners: unknown[] = []
    let reads = 0
    mockWindows('main')
    mockIPC((cmd, args) => {
      if (cmd === 'plugin:event|listen' && isEvent(args, 'tauri://resize')) {
        resizeListeners.push(args)
      }
      if (cmd === 'plugin:window|is_fullscreen' || cmd === 'plugin:window|is_maximized') reads++
      return cmd.startsWith('plugin:window|is_') ? false : 0
    })

    startWindow()

    await vi.waitFor(() => expect(reads).toBe(2))
    expect(resizeListeners).toHaveLength(1)
  })

  it('shows fullscreen as the latest read says, whatever order the reads answer in', async () => {
    const answers: ((fullscreen: boolean) => void)[] = []
    mockWindows('main')
    mockIPC(
      cmd =>
        cmd === 'plugin:window|is_fullscreen'
          ? new Promise<boolean>(resolve => answers.push(resolve))
          : undefined,
      { shouldMockEvents: true }
    )

    startChrome(true)
    await vi.waitFor(() => expect(answers).toHaveLength(1))
    // The window leaves fullscreen while the first read is out.
    await emit('tauri://resize', { width: 1280, height: 800 })
    await vi.waitFor(() => expect(answers).toHaveLength(2))
    answers[1](false)
    answers[0](true)

    await new Promise(resolve => setTimeout(resolve))
    expect(root.classList.contains('fullscreen')).toBe(false)
  })

  it('follows a fullscreen restore that lands right after the first read', async () => {
    let fullscreen = false
    let reads = 0
    mockWindows('main')
    mockIPC(
      cmd => {
        if (cmd !== 'plugin:window|is_fullscreen') return undefined
        reads++
        const answer = fullscreen
        if (reads === 1) {
          // The desktop app enters fullscreen just after the window shows.
          fullscreen = true
          void emit('tauri://resize', { width: 1920, height: 1080 })
        }
        return answer
      },
      { shouldMockEvents: true }
    )

    startChrome(true)

    // The window API loads on demand and each read is a round trip: wait for what they show.
    await vi.waitFor(() => expect(root.classList.contains('fullscreen')).toBe(true))
    expect(reads).toBeGreaterThan(1)
  })
})

function isEvent(args: unknown, name: string): boolean {
  return typeof args === 'object' && args !== null && 'event' in args && args.event === name
}
