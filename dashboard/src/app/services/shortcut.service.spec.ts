import { Component, computed, signal } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { Router, provideRouter } from '@angular/router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { ShortcutHelpDialogComponent } from '../components/shortcut/shortcut-help-dialog.component'
import { SHORTCUTS, type ShortcutEntry } from '../models/shortcuts'
import { ShortcutPreferences } from './shortcut-preferences.service'
import {
  HAND_OFF_LIMIT_MS,
  REVEAL_DELAY_MS,
  SHORTCUT_HELP_DIALOG_ID,
  SHORTCUT_PLATFORM,
  SHORTCUT_SCOPE,
  SHORTCUT_TABLE,
  ShortcutService,
} from './shortcut.service'
import type { ShortcutPlatform } from './shortcut.service'

@Component({ template: '' })
class BlankComponent {}

async function setup(
  platform: ShortcutPlatform = { os: 'mac', build: 'desktop' },
  url = '/timeline',
  table: readonly ShortcutEntry[] = SHORTCUTS
): Promise<ShortcutService> {
  TestBed.configureTestingModule({
    providers: [
      provideRouter([
        { path: 'timeline', component: BlankComponent, data: { [SHORTCUT_SCOPE]: 'timeline' } },
        { path: 'day', component: BlankComponent, data: { [SHORTCUT_SCOPE]: 'calendar' } },
        { path: 'settings', component: BlankComponent },
      ]),
      { provide: SHORTCUT_PLATFORM, useValue: platform },
      { provide: SHORTCUT_TABLE, useValue: table },
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

const BROWSER: ShortcutPlatform = { os: 'mac', build: 'browser' }

/** A page over the route (the details, the phone's Settings tab), on top while `top` says. */
function page(
  scope: 'global' | 'details',
  top: () => boolean = () => true
): { kind: 'page'; scope: 'global' | 'details'; onTop: () => boolean } {
  return { kind: 'page', scope, onTop: top }
}

describe('ShortcutService', () => {
  afterEach(() => {
    document.body.replaceChildren()
    localStorage.clear()
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
    const close = shortcuts.openLayer(page('global'))
    expect(press('/').defaultPrevented).toBe(false)
    expect(shortcuts.choose('timeline.filter')).toBe(false)
    close()
    expect(press('/').defaultPrevented).toBe(true)
    expect(run).toHaveBeenCalledOnce()
  })

  it('puts the scope of the page on top on screen, in place of the route’s', async () => {
    const shortcuts = await setup(BROWSER, '/day')
    expect(shortcuts.activeScope()).toBe('calendar')

    // The details over the week, then the phone's Settings tab opened under them.
    const detailsOnTop = signal(true)
    const closeDetails = shortcuts.openLayer(page('details', detailsOnTop))
    expect(shortcuts.activeScope()).toBe('details')
    const closeSettings = shortcuts.openLayer(page('global', () => !detailsOnTop()))
    expect(shortcuts.activeScope()).toBe('details')

    detailsOnTop.set(false)
    expect(shortcuts.activeScope()).toBe('global')
    closeSettings()
    closeDetails()
    expect(shortcuts.activeScope()).toBe('calendar')

    // No page on top (a dialog over both): the latest page's scope, under which keys wait.
    shortcuts.openLayer(page('details', () => false))
    expect(shortcuts.activeScope()).toBe('details')
  })

  it('steps the open details with P and N, not the calendar behind them, and Today does nothing', async () => {
    const shortcuts = await setup(BROWSER, '/day')
    const calendar = { previous: vi.fn(), next: vi.fn(), today: vi.fn() }
    const details = { previous: vi.fn(), next: vi.fn() }
    shortcuts.bind('calendar.previous', calendar.previous)
    shortcuts.bind('calendar.next', calendar.next)
    shortcuts.bind('calendar.today', calendar.today)

    expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(true)
    expect(calendar.next).toHaveBeenCalledOnce()

    const detailsOnTop = signal(true)
    shortcuts.openLayer(page('details', detailsOnTop))
    shortcuts.bind('details.previous', details.previous, detailsOnTop)
    shortcuts.bind('details.next', details.next, detailsOnTop)
    expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(true)
    expect(press('p', { code: 'KeyP' }).defaultPrevented).toBe(true)
    expect(press('t', { code: 'KeyT' }).defaultPrevented).toBe(false)
    expect(details.next).toHaveBeenCalledOnce()
    expect(details.previous).toHaveBeenCalledOnce()
    expect(calendar.next).toHaveBeenCalledOnce()
    expect(calendar.previous).not.toHaveBeenCalled()
    expect(calendar.today).not.toHaveBeenCalled()

    // The phone's Settings tab opened while the details show: they stay on top, and theirs.
    shortcuts.openLayer(page('global', () => !detailsOnTop()))
    expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(true)
    expect(details.next).toHaveBeenCalledTimes(2)
  })

  it('holds the calendar’s keys only on a calendar view', async () => {
    const shortcuts = await setup(BROWSER, '/timeline')
    const today = vi.fn()
    shortcuts.bind('calendar.today', today)

    expect(press('t', { code: 'KeyT' }).defaultPrevented).toBe(false)
    await TestBed.inject(Router).navigateByUrl('/day')
    expect(press('t', { code: 'KeyT' }).defaultPrevented).toBe(true)
    await TestBed.inject(Router).navigateByUrl('/settings')
    expect(press('t', { code: 'KeyT' }).defaultPrevented).toBe(false)
    expect(today).toHaveBeenCalledOnce()
  })

  it('steps once for a held key', async () => {
    const shortcuts = await setup(BROWSER, '/day')
    const next = vi.fn()
    shortcuts.bind('calendar.next', next)

    press('n', { code: 'KeyN' })
    press('n', { code: 'KeyN', repeat: true })
    press('n', { code: 'KeyN', repeat: true })

    expect(next).toHaveBeenCalledOnce()
  })

  it('takes R as the layout labels it, and where a Latin keyboard has it on a Cyrillic layout', async () => {
    const shortcuts = await setup(BROWSER)
    const collect = vi.fn()
    shortcuts.bind('app.collect', collect)

    // Dvorak: the key printed R (QWERTY's O) collects, the one printed P (QWERTY's R) does not.
    expect(press('r', { code: 'KeyO' }).defaultPrevented).toBe(true)
    expect(press('p', { code: 'KeyR' }).defaultPrevented).toBe(false)
    // Cyrillic: к has no Latin letter, so its key's place decides.
    expect(press('к', { code: 'KeyR' }).defaultPrevented).toBe(true)
    expect(collect).toHaveBeenCalledTimes(2)
  })

  it('counts keys as typing while a command hands on to a field, until it has', async () => {
    const shortcuts = await setup(BROWSER)
    const collect = vi.fn()
    const close = vi.fn()
    const filter = vi.fn()
    shortcuts.bind('app.collect', collect)
    shortcuts.bind('layer.close', close)
    shortcuts.bind('timeline.filter', filter)

    shortcuts.expectHandOff('timeline.filter')
    expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(false)
    // Escape works while typing, so it works here too.
    expect(press('Escape').defaultPrevented).toBe(true)
    expect(shortcuts.handOff('timeline.filter')).toBe(true)
    expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(true)

    // Called off (the view did not change): the keys are the app's again.
    const cancel = shortcuts.expectHandOff('timeline.filter')
    expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(false)
    cancel()
    expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(true)

    // A hand-off that never comes ends at the limit.
    const now = vi.spyOn(performance, 'now')
    now.mockReturnValue(1_000)
    shortcuts.expectHandOff('timeline.filter')
    now.mockReturnValue(1_000 + HAND_OFF_LIMIT_MS - 1)
    expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(false)
    now.mockReturnValue(1_000 + HAND_OFF_LIMIT_MS)
    expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(true)
    now.mockRestore()

    expect(collect).toHaveBeenCalledTimes(3)
    expect(close).toHaveBeenCalledOnce()
    expect(filter).toHaveBeenCalledOnce()
  })

  it('counts a typeahead widget as typing: a select’s letters pick its options', async () => {
    const shortcuts = await setup(BROWSER)
    const collect = vi.fn()
    shortcuts.bind('app.collect', collect)
    // A Material select: its host is a combobox that keeps focus, its open list a listbox.
    const select = document.createElement('div')
    select.setAttribute('role', 'combobox')
    select.tabIndex = 0
    const list = document.createElement('div')
    list.setAttribute('role', 'listbox')
    const option = document.createElement('div')
    option.setAttribute('role', 'option')
    list.append(option)
    document.body.append(select, list)

    expect(press('r', { code: 'KeyR' }, select).defaultPrevented).toBe(false)
    expect(press('r', { code: 'KeyR' }, option).defaultPrevented).toBe(false)
    expect(collect).not.toHaveBeenCalled()
    expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(true)
    expect(collect).toHaveBeenCalledOnce()
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

  it('leaves the browser its modified keys: ⌘R, ⌘F, ⌘1, ⌘, and Alt+← are not the app’s in a tab', async () => {
    const shortcuts = await setup({ os: 'other', build: 'browser' })
    for (const id of [
      'app.collect',
      'app.filter',
      'app.back',
      'app.settings',
      'view.day',
      'timeline.filter',
      'app.help',
      'layer.close',
    ] as const) {
      shortcuts.bind(id, vi.fn())
    }

    expect(press('r', { ctrlKey: true }).defaultPrevented).toBe(false)
    expect(press('F5').defaultPrevented).toBe(false)
    expect(press('f', { ctrlKey: true }).defaultPrevented).toBe(false)
    expect(press('2', { ctrlKey: true }).defaultPrevented).toBe(false)
    expect(press(',', { ctrlKey: true }).defaultPrevented).toBe(false)
    expect(press('ArrowLeft', { altKey: true }).defaultPrevented).toBe(false)
    expect(press('/').defaultPrevented).toBe(true)
    expect(press('?', { shiftKey: true }).defaultPrevented).toBe(true)
    expect(press('Escape').defaultPrevented).toBe(true)
  })

  it.each([
    { os: 'mac', build: 'browser' },
    { os: 'other', build: 'browser' },
    { os: 'mac', build: 'desktop' },
    { os: 'other', build: 'desktop' },
  ] as const)('takes R, 2 and , as Collect, Day and Settings: $os $build', async platform => {
    const shortcuts = await setup(platform)
    const collect = vi.fn()
    const day = vi.fn()
    const settings = vi.fn()
    shortcuts.bind('app.collect', collect)
    shortcuts.bind('view.day', day)
    shortcuts.bind('app.settings', settings)

    expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(true)
    expect(press('2', { code: 'Digit2' }).defaultPrevented).toBe(true)
    expect(press(',', { code: 'Comma' }).defaultPrevented).toBe(true)
    // Not from a field: they type there.
    expect(press('r', { code: 'KeyR' }, field()).defaultPrevented).toBe(false)

    expect(collect).toHaveBeenCalledOnce()
    expect(day).toHaveBeenCalledOnce()
    expect(settings).toHaveBeenCalledOnce()
  })

  it('turns every single key off with the setting, and keeps the modified ones and Escape', async () => {
    const shortcuts = await setup({ os: 'mac', build: 'desktop' })
    const runs = {
      collect: vi.fn(),
      day: vi.fn(),
      filter: vi.fn(),
      help: vi.fn(),
      close: vi.fn(),
    }
    shortcuts.bind('app.collect', runs.collect)
    shortcuts.bind('view.day', runs.day)
    shortcuts.bind('timeline.filter', runs.filter)
    shortcuts.bind('app.help', runs.help)
    shortcuts.bind('layer.close', runs.close)
    expect(shortcuts.hint('timeline.filter')).toBe('/')

    // Turned off elsewhere (the settings), read at the next key.
    TestBed.inject(ShortcutPreferences).setCharacterKeys('off')

    for (const [key, code] of [
      ['r', 'KeyR'],
      ['2', 'Digit2'],
      ['/', 'Slash'],
      ['?', 'Slash'],
    ]) {
      expect(press(key, { code }).defaultPrevented, key).toBe(false)
    }
    expect(press('r', { metaKey: true, code: 'KeyR' }).defaultPrevented).toBe(true)
    expect(press('Escape').defaultPrevented).toBe(true)
    expect(runs.collect).toHaveBeenCalledOnce()
    expect(runs.close).toHaveBeenCalledOnce()
    expect(runs.day).not.toHaveBeenCalled()
    expect(runs.filter).not.toHaveBeenCalled()
    expect(runs.help).not.toHaveBeenCalled()

    // What names the keys drops them too.
    expect(shortcuts.hint('app.collect')).toBe('⌘R')
    expect(shortcuts.ariaKeys('app.collect')).toBe('Meta+R')
    expect(shortcuts.hint('timeline.filter')).toBeNull()
    expect(shortcuts.ariaKeys('app.help')).toBeNull()
  })

  it('keeps the setting on the device, for the next start and other tabs', async () => {
    TestBed.inject(ShortcutPreferences).setCharacterKeys('off')
    expect(localStorage.getItem('character-shortcuts')).toBe('off')
    TestBed.resetTestingModule()

    const shortcuts = await setup(BROWSER)
    expect(shortcuts.hint('app.collect')).toBeNull()
    TestBed.resetTestingModule()

    // Anything else stored reads as on.
    localStorage.setItem('character-shortcuts', 'sometimes')
    expect((await setup(BROWSER)).hint('app.collect')).toBe('R')
  })

  it('names the keys with ⌘ on macOS and Ctrl elsewhere, and none the build does not have', async () => {
    const mac = await setup({ os: 'mac', build: 'desktop' })
    expect(mac.hint('app.collect')).toBe('⌘R')
    expect(mac.ariaKeys('app.collect')).toBe('Meta+R R')
    expect(mac.ariaKeys('app.back')).toBe('Meta+[')
    TestBed.resetTestingModule()

    const windows = await setup({ os: 'other', build: 'desktop' })
    expect(windows.hint('app.collect')).toBe('Ctrl+R')
    expect(windows.ariaKeys('app.collect')).toBe('Control+R F5 R')
    expect(windows.ariaKeys('app.back')).toBe('Alt+ArrowLeft')

    // The app's tag shows its modified chord; the browser has only the key alone.
    expect(windows.hint('view.day')).toBe('Ctrl+2')
    expect(windows.ariaKeys('view.day')).toBe('Control+2 2')

    TestBed.resetTestingModule()
    const browser = await setup({ os: 'mac', build: 'browser' })
    expect(browser.hint('app.collect')).toBe('R')
    expect(browser.ariaKeys('app.collect')).toBe('R')
    expect(browser.hint('app.back')).toBeNull()
    expect(browser.ariaKeys('app.back')).toBeNull()
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
  })

  it('resolves a chord to the entry of the scope on screen as the route changes', async () => {
    // The Timeline's own ⌘F beside the global one, as Work items' search will have.
    const timelineFind: ShortcutEntry = {
      id: 'timeline.filter',
      label: 'Find in the Timeline',
      scope: 'timeline',
      chords: [{ keys: 'mod+f', whileTyping: true, menuAccelerator: true }],
    }
    const shortcuts = await setup(undefined, '/timeline', [...SHORTCUTS, timelineFind])
    const find = shortcuts.resolve('mod+f')
    const collect = shortcuts.resolve('mod+r')

    expect(find()?.label).toBe('Find in the Timeline')
    expect(collect()?.id).toBe('app.collect')
    await TestBed.inject(Router).navigateByUrl('/day')
    expect(find()?.id).toBe('app.filter')
    shortcuts.openLayer(page('global'))
    await TestBed.inject(Router).navigateByUrl('/timeline')
    expect(find()?.id).toBe('app.filter')
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

  describe('revealing the key tags: Shift held on its own', () => {
    afterEach(() => {
      vi.useRealTimers()
    })

    /** The service, with the delay's timer in the spec's hands. */
    async function revealing(): Promise<ShortcutService> {
      const shortcuts = await setup(BROWSER)
      vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })
      return shortcuts
    }

    function release(key = 'Shift'): void {
      document.body.dispatchEvent(new KeyboardEvent('keyup', { key, bubbles: true }))
    }

    function holdShift(target: EventTarget = document.body): void {
      press('Shift', { shiftKey: true }, target)
    }

    it('shows them once Shift has been held past the delay, its repeats changing nothing', async () => {
      const shortcuts = await revealing()
      expect(shortcuts.revealKeys()).toBe(false)

      holdShift()
      vi.advanceTimersByTime(REVEAL_DELAY_MS - 50)
      // Auto-repeat neither starts the delay again nor ends it.
      press('Shift', { shiftKey: true, repeat: true })
      vi.advanceTimersByTime(49)
      expect(shortcuts.revealKeys()).toBe(false)
      vi.advanceTimersByTime(1)
      expect(shortcuts.revealKeys()).toBe(true)
      press('Shift', { shiftKey: true, repeat: true })
      expect(shortcuts.revealKeys()).toBe(true)

      release()
      expect(shortcuts.revealKeys()).toBe(false)
    })

    it('never shows them for a Shift on its way to a letter or a click', async () => {
      const shortcuts = await revealing()
      holdShift()
      press('R', { shiftKey: true, code: 'KeyR' })
      vi.advanceTimersByTime(REVEAL_DELAY_MS)
      expect(shortcuts.revealKeys()).toBe(false)

      release()
      holdShift()
      document.body.dispatchEvent(new Event('pointerdown', { bubbles: true }))
      vi.advanceTimersByTime(REVEAL_DELAY_MS)
      expect(shortcuts.revealKeys()).toBe(false)
    })

    it('hides them at another key, a click, and as the window or the page goes away', async () => {
      const shortcuts = await revealing()
      const ends: readonly (() => void)[] = [
        () => press('R', { shiftKey: true, code: 'KeyR' }),
        () => document.body.dispatchEvent(new Event('pointerdown', { bubbles: true })),
        () => window.dispatchEvent(new Event('blur')),
        () => document.dispatchEvent(new Event('visibilitychange')),
      ]
      for (const end of ends) {
        holdShift()
        vi.advanceTimersByTime(REVEAL_DELAY_MS)
        expect(shortcuts.revealKeys()).toBe(true)
        end()
        expect(shortcuts.revealKeys()).toBe(false)
        // Held on, Shift shows nothing again until it is pressed again.
        vi.advanceTimersByTime(REVEAL_DELAY_MS)
        expect(shortcuts.revealKeys()).toBe(false)
      }
    })

    it('shows none while typing, with another modifier, or inside inert content', async () => {
      const shortcuts = await revealing()
      const input = field()
      const select = document.createElement('div')
      select.setAttribute('role', 'combobox')
      const inert = document.createElement('div')
      inert.setAttribute('inert', '')
      const inside = document.createElement('button')
      inert.append(inside)
      document.body.append(select, inert)

      for (const target of [input, select, inside]) {
        holdShift(target)
        vi.advanceTimersByTime(REVEAL_DELAY_MS)
        expect(shortcuts.revealKeys()).toBe(false)
        release()
      }
      press('Shift', { shiftKey: true, metaKey: true })
      vi.advanceTimersByTime(REVEAL_DELAY_MS)
      expect(shortcuts.revealKeys()).toBe(false)
    })

    it('shows none under a modal layer or a dialog, the help included', async () => {
      const shortcuts = await revealing()
      holdShift()
      vi.advanceTimersByTime(REVEAL_DELAY_MS)
      expect(shortcuts.revealKeys()).toBe(true)

      const closePicker = shortcuts.openLayer({ kind: 'modal' })
      expect(shortcuts.revealKeys()).toBe(false)
      closePicker()
      expect(shortcuts.revealKeys()).toBe(true)

      vi.useRealTimers()
      const dialogs = TestBed.inject(MatDialog)
      dialogs.open(ShortcutHelpDialogComponent, { id: SHORTCUT_HELP_DIALOG_ID })
      await vi.waitFor(() => expect(shortcuts.revealKeys()).toBe(false))
      dialogs.closeAll()
      await vi.waitFor(() => expect(shortcuts.revealKeys()).toBe(true))
    })
  })
})
