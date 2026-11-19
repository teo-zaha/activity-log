import { TestBed } from '@angular/core/testing'
import { afterAll, afterEach, beforeAll, describe, expect, it } from 'vitest'
import { APP_CONTEXT_MENU } from '../utils/context-menu'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'
import { WindowChromeService } from './window-chrome.service'

/** Where the selected text lies on screen; jsdom lays nothing out. */
let selectedRects: DOMRect[] = []

function startChrome(tauri: boolean): void {
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
