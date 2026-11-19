/** Input types that take text, where the right-click menu is the system's Cut, Copy and Paste. */
const TEXT_INPUT_TYPES: ReadonlySet<string> = new Set([
  'text',
  'search',
  'email',
  'url',
  'tel',
  'password',
  'number',
])

/**
 * Sent on `document` once the app limits the right-click menu: `index.html` then drops the
 * blocker that covers the splash.
 */
export const APP_CONTEXT_MENU = 'activity-log:context-menu'

/** Selected text as it lies on screen, one rectangle per line; empty when nothing is selected. */
export type SelectionRects = readonly DOMRectReadOnly[]

/** What `selectionRects` reads of the document's selection; a `Selection` is one. */
export interface SelectionArea {
  readonly isCollapsed: boolean
  readonly rangeCount: number
  getRangeAt(index: number): { getClientRects(): ArrayLike<DOMRectReadOnly> }
}

export function selectionRects(selection: SelectionArea | null): SelectionRects {
  if (!selection || selection.isCollapsed) return []
  const rects: DOMRectReadOnly[] = []
  for (let index = 0; index < selection.rangeCount; index++) {
    rects.push(...Array.from(selection.getRangeAt(index).getClientRects()))
  }
  return rects
}

/**
 * Whether a right-click keeps the web view's own menu in the desktop app: in a text field, or over
 * selected text, where it offers Cut, Copy and Paste. Everywhere else the app shows no menu.
 * `selected` is the selection from before the click: WebKit selects the word under a right-click
 * before `contextmenu`, so the selection then always lies under the pointer.
 */
export function keepsNativeMenu(event: MouseEvent, selected: SelectionRects): boolean {
  return isTextField(event.composedPath()[0] ?? event.target) || overSelection(event, selected)
}

function isTextField(target: EventTarget | null): boolean {
  if (target instanceof HTMLTextAreaElement) return true
  if (target instanceof HTMLInputElement) return TEXT_INPUT_TYPES.has(target.type)
  return target instanceof HTMLElement && target.isContentEditable
}

function overSelection(event: MouseEvent, selected: SelectionRects): boolean {
  // The menu key and Shift+F10 open it with no pointer: Chromium (WebView2) sends button -1 then,
  // and a point that need not lie on the selection. Any selection counts.
  if (event.button < 0) return selected.length > 0
  return selected.some(rect => contains(rect, event.clientX, event.clientY))
}

function contains(rect: DOMRectReadOnly, x: number, y: number): boolean {
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom
}
