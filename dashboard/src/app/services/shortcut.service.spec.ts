import { Component, computed, signal } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { Router, provideRouter } from '@angular/router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ShortcutHelpDialogComponent } from '../components/shortcut/shortcut-help-dialog.component'
import {
  SHORTCUT_HELP_DIALOG_ID,
  SHORTCUT_PLATFORM,
  SHORTCUT_SCOPE,
  ShortcutService,
} from './shortcut.service'
import type { ShortcutPlatform } from './shortcut.service'

@Component({ template: '' })
class BlankComponent {}

async function setup(
  platform: ShortcutPlatform = { os: 'mac', build: 'desktop' },
  url = '/timeline'
): Promise<ShortcutService> {
  TestBed.configureTestingModule({
    providers: [
      provideRouter([
        { path: 'timeline', component: BlankComponent, data: { [SHORTCUT_SCOPE]: 'timeline' } },
        { path: 'day', component: BlankComponent },
      ]),
      { provide: SHORTCUT_PLATFORM, useValue: platform },
    ],
  })
  const shortcuts = TestBed.inject(ShortcutService)
  await TestBed.inject(Router).navigateByUrl(url)
  return shortcuts
}

/** A key press on `target`, as the browser sends it: bubbling and cancelable. */
function press(
  key: string,
  init: KeyboardEventInit = {},
  target: EventTarget = document.body
): KeyboardEvent {
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init })
  target.dispatchEvent(event)
  return event
}

function field(type = 'text'): HTMLInputElement {
  const input = document.createElement('input')
  input.type = type
  document.body.append(input)
  return input
}

describe('ShortcutService', () => {
  afterEach(() => {
    document.body.replaceChildren()
  })

  it('runs the binding of a pressed chord and marks the key handled', async () => {
    const shortcuts = await setup()
    const run = vi.fn()
    shortcuts.bind('app.collect', run)

    expect(press('r', { metaKey: true }).defaultPrevented).toBe(true)
    expect(run).toHaveBeenCalledOnce()
  })

  it('leaves keys alone that no entry has here', async () => {
    const shortcuts = await setup({ os: 'mac', build: 'desktop' })
    const run = vi.fn()
    shortcuts.bind('app.collect', run)

    // Ctrl+R is not ⌘R on macOS, and F5 collects only elsewhere.
    expect(press('r', { ctrlKey: true }).defaultPrevented).toBe(false)
    expect(press('F5').defaultPrevented).toBe(false)
    expect(run).not.toHaveBeenCalled()
  })

  it('collects with Ctrl+R and F5 off macOS, also while typing', async () => {
    const shortcuts = await setup({ os: 'other', build: 'desktop' })
    const run = vi.fn()
    shortcuts.bind('app.collect', run)
    const input = field()

    expect(press('r', { ctrlKey: true }, input).defaultPrevented).toBe(true)
    expect(press('F5', {}, input).defaultPrevented).toBe(true)
    expect(run).toHaveBeenCalledTimes(2)
  })

  it('fires no character key in a field, select or editable content', async () => {
    const shortcuts = await setup()
    const filter = vi.fn()
    const help = vi.fn()
    shortcuts.bind('timeline.filter', filter)
    shortcuts.bind('app.help', help)
    const editable = document.createElement('div')
    editable.setAttribute('contenteditable', '')
    document.body.append(editable)

    const select = document.createElement('select')
    select.append(new Option('One'))
    for (const target of [
      field(),
      field('search'),
      document.createElement('textarea'),
      select,
      editable,
    ]) {
      document.body.append(target)
      expect(press('/', {}, target).defaultPrevented).toBe(false)
      expect(press('?', { shiftKey: true }, target).defaultPrevented).toBe(false)
    }
    expect(filter).not.toHaveBeenCalled()
    expect(help).not.toHaveBeenCalled()

    // A checkbox does not type: the key is the app's there.
    press('/', {}, field('checkbox'))
    expect(filter).toHaveBeenCalledOnce()
  })

  it('lets the nearest explicit editing boundary decide what is editable', async () => {
    const shortcuts = await setup()
    const filter = vi.fn()
    shortcuts.bind('timeline.filter', filter)
    const editor = document.createElement('div')
    editor.setAttribute('contenteditable', 'true')
    const text = document.createElement('span')
    const tool = document.createElement('button')
    tool.setAttribute('contenteditable', 'false')
    const label = document.createElement('span')
    tool.append(label)
    editor.append(text, tool)
    document.body.append(editor)

    expect(press('/', {}, text).defaultPrevented).toBe(false)
    expect(press('/', {}, label).defaultPrevented).toBe(true)
    expect(filter).toHaveBeenCalledOnce()
  })

  it('ignores the key an input method reports as 229 while it composes', async () => {
    const shortcuts = await setup()
    const run = vi.fn()
    shortcuts.bind('timeline.filter', run)
    const event = new KeyboardEvent('keydown', { key: '/', bubbles: true, cancelable: true })
    Object.defineProperty(event, 'keyCode', { value: 229 })

    document.body.dispatchEvent(event)

    expect(event.defaultPrevented).toBe(false)
    expect(run).not.toHaveBeenCalled()
  })

  it('applies the typing rule to a menu accelerator, not to a menu item chosen by a click', async () => {
    const shortcuts = await setup()
    const back = vi.fn()
    const collect = vi.fn()
    const exportMarkdown = vi.fn()
    shortcuts.bind('app.back', back)
    shortcuts.bind('app.collect', collect)
    shortcuts.bind('app.export-markdown', exportMarkdown)
    field().focus()

    // Back is not a typing chord: from the menu it waits while a field has focus; Collect is one.
    expect(shortcuts.triggerChord('mod+[')).toBe(false)
    expect(shortcuts.triggerChord('mod+r')).toBe(true)
    expect(back).not.toHaveBeenCalled()
    expect(collect).toHaveBeenCalledOnce()
    // An item without a key is only ever clicked: it runs while a field has focus.
    expect(shortcuts.choose('app.export-markdown')).toBe(true)
    expect(exportMarkdown).toHaveBeenCalledOnce()

    // A command that hands on to another (⌘F to the filter) is past the typing rule already.
    const filter = vi.fn()
    shortcuts.bind('timeline.filter', filter)
    expect(shortcuts.handOff('timeline.filter')).toBe(true)
    expect(filter).toHaveBeenCalledOnce()
  })

  it('leaves the keyboard to a modal layer, such as an open date picker, for as long as it is open', async () => {
    const shortcuts = await setup()
    const filter = vi.fn()
    const help = vi.fn()
    const close = vi.fn()
    shortcuts.bind('timeline.filter', filter)
    shortcuts.bind('app.help', help)
    shortcuts.bind('layer.close', close)

    const closePicker = shortcuts.openLayer({ kind: 'modal' })
    expect(press('/').defaultPrevented).toBe(false)
    expect(press('?').defaultPrevented).toBe(false)
    expect(press('Escape').defaultPrevented).toBe(false)
    expect(shortcuts.choose('timeline.filter')).toBe(false)
    closePicker()

    expect(press('/').defaultPrevented).toBe(true)
    expect(press('Escape').defaultPrevented).toBe(true)
    expect(filter).toHaveBeenCalledOnce()
    expect(help).not.toHaveBeenCalled()
    expect(close).toHaveBeenCalledOnce()
  })

  it('ignores a key mid-composition, held down, handled, or pressed in an inert subtree', async () => {
    const shortcuts = await setup()
    const run = vi.fn()
    shortcuts.bind('timeline.filter', run)

    press('/', { isComposing: true })
    press('/', { repeat: true })
    const handled = new KeyboardEvent('keydown', { key: '/', bubbles: true, cancelable: true })
    handled.preventDefault()
    document.body.dispatchEvent(handled)
    const inert = document.createElement('div')
    inert.setAttribute('inert', '')
    const button = document.createElement('button')
    inert.append(button)
    document.body.append(inert)
    press('/', {}, button)

    expect(run).not.toHaveBeenCalled()
  })

  it('holds a view’s keys only while its route is on screen and no layer hides it', async () => {
    const shortcuts = await setup()
    const run = vi.fn()
    shortcuts.bind('timeline.filter', run)

    await TestBed.inject(Router).navigateByUrl('/day')
    expect(press('/').defaultPrevented).toBe(false)
    await TestBed.inject(Router).navigateByUrl('/timeline')
    const close = shortcuts.openLayer({ kind: 'page', hidesRoute: true })
    expect(press('/').defaultPrevented).toBe(false)
    expect(shortcuts.choose('timeline.filter')).toBe(false)
    close()
    expect(press('/').defaultPrevented).toBe(true)
    expect(run).toHaveBeenCalledOnce()

    // A layer that leaves the view in sight (the details beside it) keeps its keys.
    shortcuts.openLayer({ kind: 'page', hidesRoute: false })
    expect(shortcuts.activeScope()).toBe('timeline')
  })

  it('tries the latest binding first, then the ones before, then the fallback', async () => {
    const shortcuts = await setup()
    const calls: string[] = []
    shortcuts.fallback('layer.close', () => void calls.push('fallback'))
    shortcuts.bind('layer.close', () => void calls.push('outer'))
    const inner = shortcuts.bind('layer.close', () => {
      calls.push('inner declines')
      return false
    })
    shortcuts.bind(
      'layer.close',
      () => void calls.push('off'),
      () => false
    )

    press('Escape')
    inner()
    press('Escape')

    expect(calls).toEqual(['inner declines', 'outer', 'outer'])
  })

  it('runs the fallback when no binding takes the key, and nothing when it declines', async () => {
    const shortcuts = await setup()
    let enabled = true
    shortcuts.fallback('app.collect', vi.fn(), () => enabled)

    expect(press('r', { metaKey: true }).defaultPrevented).toBe(true)
    enabled = false
    expect(press('r', { metaKey: true }).defaultPrevented).toBe(false)
    expect(() => shortcuts.fallback('app.collect', vi.fn())).toThrow()
  })

  it('takes Escape in a field to close a layer, unless the field handled it', async () => {
    const shortcuts = await setup()
    const close = vi.fn()
    shortcuts.bind('layer.close', close)
    const input = field()

    expect(press('Escape', {}, input).defaultPrevented).toBe(true)
    input.addEventListener('keydown', event => event.preventDefault())
    press('Escape', {}, input)

    expect(close).toHaveBeenCalledOnce()
  })

  it('goes back neither while typing nor from the platform’s other chord', async () => {
    const shortcuts = await setup({ os: 'other', build: 'desktop' })
    const back = vi.fn()
    shortcuts.bind('app.back', back)

    expect(press('ArrowLeft', { altKey: true }, field()).defaultPrevented).toBe(false)
    expect(press('[', { ctrlKey: true }).defaultPrevented).toBe(false)
    expect(press('ArrowLeft', { altKey: true }).defaultPrevented).toBe(true)
    expect(back).toHaveBeenCalledOnce()
  })

  it('leaves every key to a dialog that is open, and only `?` to the help', async () => {
    const shortcuts = await setup()
    const close = vi.fn()
    const collect = vi.fn()
    const help = vi.fn()
    shortcuts.bind('layer.close', close)
    shortcuts.bind('app.collect', collect)
    shortcuts.bind('app.help', help)
    const dialogs = TestBed.inject(MatDialog)

    const other = dialogs.open(BlankComponent)
    press('Escape')
    press('r', { metaKey: true })
    press('?')
    other.close()
    await vi.waitFor(() => expect(dialogs.openDialogs).toHaveLength(0))

    dialogs.open(ShortcutHelpDialogComponent, { id: SHORTCUT_HELP_DIALOG_ID })
    press('r', { metaKey: true })
    press('?')

    expect(close).not.toHaveBeenCalled()
    expect(collect).not.toHaveBeenCalled()
    expect(help).toHaveBeenCalledOnce()
  })

  it('leaves the browser its own keys: only `/`, `?` and a layer’s Escape are the app’s', async () => {
    const shortcuts = await setup({ os: 'other', build: 'browser' })
    for (const id of [
      'app.collect',
      'app.filter',
      'app.back',
      'timeline.filter',
      'app.help',
      'layer.close',
    ] as const) {
      shortcuts.bind(id, vi.fn())
    }

    expect(press('r', { ctrlKey: true }).defaultPrevented).toBe(false)
    expect(press('F5').defaultPrevented).toBe(false)
    expect(press('f', { ctrlKey: true }).defaultPrevented).toBe(false)
    expect(press('ArrowLeft', { altKey: true }).defaultPrevented).toBe(false)
    expect(press('/').defaultPrevented).toBe(true)
    expect(press('?', { shiftKey: true }).defaultPrevented).toBe(true)
    expect(press('Escape').defaultPrevented).toBe(true)
  })

  it('names the keys with ⌘ on macOS and Ctrl elsewhere, and none the build does not have', async () => {
    const mac = await setup({ os: 'mac', build: 'desktop' })
    expect(mac.hint('app.collect')).toBe('⌘R')
    expect(mac.ariaKeys('app.collect')).toBe('Meta+R')
    expect(mac.ariaKeys('app.back')).toBe('Meta+[')
    TestBed.resetTestingModule()

    const windows = await setup({ os: 'other', build: 'desktop' })
    expect(windows.hint('app.collect')).toBe('Ctrl+R')
    expect(windows.ariaKeys('app.collect')).toBe('Control+R F5')
    expect(windows.ariaKeys('app.back')).toBe('Alt+ArrowLeft')
    TestBed.resetTestingModule()

    const browser = await setup({ os: 'mac', build: 'browser' })
    expect(browser.hint('app.collect')).toBeNull()
    expect(browser.ariaKeys('app.collect')).toBeNull()
    expect(browser.hint('timeline.filter')).toBe('/')
  })

  it('leaves a menu accelerator to the macOS menu only once the menu is installed', async () => {
    const shortcuts = await setup({ os: 'mac', build: 'desktop' })
    const collect = vi.fn()
    const close = vi.fn()
    shortcuts.bind('app.collect', collect)
    shortcuts.bind('layer.close', close)

    // Before (or after a failed installation) the page takes ⌘R.
    expect(press('r', { metaKey: true }).defaultPrevented).toBe(true)
    shortcuts.menuOwnsAccelerators.set(true)
    expect(press('r', { metaKey: true }).defaultPrevented).toBe(false)
    // Escape is no accelerator: the page keeps it.
    expect(press('Escape').defaultPrevented).toBe(true)
    shortcuts.menuOwnsAccelerators.set(false)
    expect(press('r', { metaKey: true }).defaultPrevented).toBe(true)

    expect(collect).toHaveBeenCalledTimes(2)
    expect(close).toHaveBeenCalledOnce()
  })

  it('runs a held menu accelerator once: its repeats from the menu run nothing', async () => {
    const shortcuts = await setup({ os: 'mac', build: 'desktop' })
    const collect = vi.fn()
    shortcuts.bind('app.collect', collect)
    shortcuts.menuOwnsAccelerators.set(true)

    // The menu runs each repeat the page leaves it: whatever key it is on this layout.
    press('r', { metaKey: true })
    expect(shortcuts.triggerChord('mod+r')).toBe(true)
    press('r', { metaKey: true, repeat: true })
    expect(shortcuts.triggerChord('mod+r')).toBe(false)
    press('ö', { metaKey: true, repeat: true })
    expect(shortcuts.triggerChord('mod+r')).toBe(false)
    document.body.dispatchEvent(new KeyboardEvent('keyup', { key: 'r', bubbles: true }))
    expect(shortcuts.triggerChord('mod+r')).toBe(true)

    // Leaving the window while holding a key lets it go too.
    press('r', { metaKey: true, repeat: true })
    expect(shortcuts.triggerChord('mod+r')).toBe(false)
    window.dispatchEvent(new Event('blur'))
    expect(shortcuts.triggerChord('mod+r')).toBe(true)

    expect(collect).toHaveBeenCalledTimes(3)
  })

  it('keeps every chord off macOS, whatever the flag says', async () => {
    const shortcuts = await setup({ os: 'other', build: 'desktop' })
    const collect = vi.fn()
    shortcuts.bind('app.collect', collect)
    shortcuts.menuOwnsAccelerators.set(true)

    expect(press('r', { ctrlKey: true }).defaultPrevented).toBe(true)
    expect(collect).toHaveBeenCalledOnce()
  })

  it('runs a chord from the menu by the scope on screen and the chord’s typing rule', async () => {
    const shortcuts = await setup()
    const back = vi.fn()
    const collect = vi.fn()
    shortcuts.bind('app.back', back)
    shortcuts.bind('app.collect', collect)
    const input = field()
    input.focus()

    // Back from the menu does nothing while typing; Collect is a typing chord.
    expect(shortcuts.triggerChord('mod+[')).toBe(false)
    expect(shortcuts.triggerChord('mod+r')).toBe(true)
    input.blur()
    expect(shortcuts.triggerChord('mod+[')).toBe(true)
    expect(back).toHaveBeenCalledOnce()
    expect(collect).toHaveBeenCalledOnce()

    // `/` is the Timeline's: from the menu it does nothing on another view.
    const filter = vi.fn()
    shortcuts.bind('timeline.filter', filter)
    await TestBed.inject(Router).navigateByUrl('/day')
    expect(shortcuts.triggerChord('/')).toBe(false)
    expect(filter).not.toHaveBeenCalled()
  })

  it('resolves a chord to the entry of the scope on screen as the route changes', async () => {
    const shortcuts = await setup()
    const slash = shortcuts.resolve('/')
    const collect = shortcuts.resolve('mod+r')

    expect(slash()?.id).toBe('timeline.filter')
    expect(collect()?.id).toBe('app.collect')
    await TestBed.inject(Router).navigateByUrl('/day')
    expect(slash()).toBeNull()
    shortcuts.openLayer({ kind: 'page', hidesRoute: true })
    await TestBed.inject(Router).navigateByUrl('/timeline')
    expect(slash()).toBeNull()
  })

  it('says, as signals change, whether an entry would run', async () => {
    const shortcuts = await setup()
    const enabled = signal(true)
    const available = computed(() => shortcuts.available('app.collect'))

    expect(available()).toBe(false)
    const unbind = shortcuts.bind('app.collect', vi.fn(), enabled)
    expect(available()).toBe(true)
    enabled.set(false)
    expect(available()).toBe(false)
    enabled.set(true)

    const closePicker = shortcuts.openLayer({ kind: 'modal' })
    expect(available()).toBe(false)
    closePicker()
    expect(available()).toBe(true)

    const dialogs = TestBed.inject(MatDialog)
    const dialog = dialogs.open(BlankComponent)
    expect(available()).toBe(false)
    dialog.close()
    await vi.waitFor(() => expect(available()).toBe(true))

    unbind()
    expect(available()).toBe(false)
    // A view's entry runs only while its view is on screen.
    shortcuts.bind('timeline.filter', vi.fn())
    const filter = computed(() => shortcuts.available('timeline.filter'))
    expect(filter()).toBe(true)
    await TestBed.inject(Router).navigateByUrl('/day')
    expect(filter()).toBe(false)
  })
})
