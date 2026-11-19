import { afterEach, describe, expect, it } from 'vitest'
import { type SelectionArea, keepsNativeMenu, selectionRects } from './context-menu'

function rightClick(target: Element, init: MouseEventInit = {}): MouseEvent {
  const event = new MouseEvent('contextmenu', { button: 2, clientX: 0, clientY: 0, ...init })
  Object.defineProperty(event, 'target', { value: target })
  return event
}

/** A selection drawn as `rects`, or a collapsed one without any. */
function selection(...rects: DOMRect[]): SelectionArea {
  return {
    isCollapsed: rects.length === 0,
    rangeCount: rects.length === 0 ? 0 : 1,
    getRangeAt: () => ({ getClientRects: () => rects }),
  }
}

/** jsdom has no `isContentEditable`; a browser derives it from `contenteditable`. */
function editable(value: boolean): HTMLElement {
  const element = document.createElement('div')
  element.setAttribute('contenteditable', String(value))
  Object.defineProperty(element, 'isContentEditable', { value })
  return element
}

function input(type: string): HTMLInputElement {
  const element = document.createElement('input')
  element.type = type
  return element
}

describe('keepsNativeMenu', () => {
  afterEach(() => document.body.replaceChildren())

  it('keeps the menu in a textarea, a text-like input and an editable element', () => {
    expect(keepsNativeMenu(rightClick(document.createElement('textarea')), [])).toBe(true)
    for (const type of ['text', 'search', 'email', 'url', 'tel', 'password', 'number']) {
      expect(keepsNativeMenu(rightClick(input(type)), []), type).toBe(true)
    }
    expect(keepsNativeMenu(rightClick(document.createElement('input')), [])).toBe(true)
    expect(keepsNativeMenu(rightClick(editable(true)), [])).toBe(true)
  })

  it('hides the menu on a checkbox, a range input, a non-editable element and a row', () => {
    expect(keepsNativeMenu(rightClick(input('checkbox')), [])).toBe(false)
    expect(keepsNativeMenu(rightClick(input('range')), [])).toBe(false)
    expect(keepsNativeMenu(rightClick(editable(false)), [])).toBe(false)
    expect(keepsNativeMenu(rightClick(document.createElement('li')), [])).toBe(false)
  })

  it('finds a text field inside an open shadow root', () => {
    const host = document.createElement('div')
    const field = input('text')
    host.attachShadow({ mode: 'open' }).append(field)
    document.body.append(host)
    let kept: boolean | undefined
    document.addEventListener(
      'contextmenu',
      event => {
        expect(event.target).toBe(host)
        kept = keepsNativeMenu(event, [])
      },
      { once: true }
    )

    field.dispatchEvent(new MouseEvent('contextmenu', { bubbles: true, composed: true, button: 2 }))
    expect(kept).toBe(true)
  })

  it('keeps the menu for a click inside the selected text only', () => {
    const paragraph = document.createElement('p')
    const selected = selectionRects(
      selection(new DOMRect(10, 10, 80, 16), new DOMRect(0, 26, 40, 16))
    )
    const at = (clientX: number, clientY: number): MouseEvent =>
      rightClick(paragraph, { clientX, clientY })

    expect(keepsNativeMenu(at(50, 18), selected)).toBe(true)
    expect(keepsNativeMenu(at(20, 30), selected)).toBe(true)
    // Elsewhere in the same paragraph, beside the selection.
    expect(keepsNativeMenu(at(120, 18), selected)).toBe(false)
    expect(keepsNativeMenu(at(60, 34), selected)).toBe(false)
    expect(keepsNativeMenu(at(50, 18), selectionRects(selection()))).toBe(false)
    expect(keepsNativeMenu(at(50, 18), selectionRects(null))).toBe(false)
  })

  it('keeps the menu for the menu key while text is selected, wherever focus is', () => {
    const row = document.createElement('li')
    const key = rightClick(row, { button: -1, clientX: 500, clientY: 500 })

    expect(keepsNativeMenu(key, selectionRects(selection(new DOMRect(10, 10, 80, 16))))).toBe(true)
    expect(keepsNativeMenu(key, [])).toBe(false)
    expect(
      keepsNativeMenu(rightClick(document.createElement('textarea'), { button: -1 }), [])
    ).toBe(true)
  })
})
