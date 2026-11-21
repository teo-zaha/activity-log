import { Component, signal } from '@angular/core'
import { ShortcutDirective } from '../../components/shortcut/shortcut.directive'
import { SHORTCUTS, type ShortcutEntry } from '../../models/shortcuts'
import { TestBed } from '@angular/core/testing'
import { Router, provideRouter } from '@angular/router'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { MenuInstall, MenuItemState, MenuItemUpdate, MenuSpec } from '../../generated/activity'
import type { ViewMode } from '../../utils/app-url'
import { ActivityViewStateService } from '../activity-view-state.service'
import { BackendFailure } from '../backend/backend-failure'
import {
  SHORTCUT_PLATFORM,
  SHORTCUT_SCOPE,
  SHORTCUT_TABLE,
  type ShortcutPlatform,
  ShortcutService,
} from '../shortcut.service'
import { NATIVE_MENU_IPC, NativeMenuService, type NativeMenuIpc } from './native-menu.service'

@Component({ template: '' })
class BlankComponent {}

/** Collect as the range form has it: a button the key clicks, disabled while loading. */
@Component({
  imports: [ShortcutDirective],
  template: `<button [appShortcut]="'app.collect'" [disabled]="loading()">Collect</button>`,
})
class CollectButtonComponent {
  readonly loading = signal(false)
}

interface Deferred<T> {
  readonly promise: Promise<T>
  resolve(value: T): void
  reject(error: unknown): void
}

function deferred<T>(): Deferred<T> {
  let resolve!: (value: T) => void
  let reject!: (error: unknown) => void
  const promise = new Promise<T>((done, fail) => {
    resolve = done
    reject = fail
  })
  return { promise, resolve, reject }
}

/** The desktop app's menu commands, answered when the spec says. */
class FakeMenu implements NativeMenuIpc {
  readonly installs: MenuSpec[] = []
  readonly updates: { generation: number; items: readonly MenuItemUpdate[] }[] = []
  readonly installed = deferred<MenuInstall>()
  /** The next update's answer; null answers at once. */
  nextUpdate: Deferred<void> | null = null
  receive: ((payload: unknown) => void) | null = null

  install(spec: MenuSpec): Promise<MenuInstall> {
    this.installs.push(spec)
    return this.installed.promise
  }

  update(generation: number, items: readonly MenuItemUpdate[]): Promise<void> {
    this.updates.push({ generation, items })
    return this.nextUpdate?.promise ?? Promise.resolve()
  }

  listen(receive: (payload: unknown) => void): Promise<() => void> {
    this.receive = receive
    return Promise.resolve(() => {
      this.receive = null
    })
  }

  /** The key of the item titled `title` in the menu installed. */
  key(title: string): number {
    const item = this.installs[0]?.sections
      .flatMap(section => section.items)
      .find(candidate => candidate.state.title === title)
    if (!item) throw new Error(`No menu item ${title}`)
    return item.key
  }

  /** What `title` shows after the latest update that changed it, else as installed. */
  shown(title: string): MenuItemState | undefined {
    const key = this.key(title)
    const updated = this.updates
      .flatMap(update => update.items)
      .filter(item => item.key === key)
      .at(-1)
    return (
      updated?.state ??
      this.installs[0]?.sections.flatMap(section => section.items).find(item => item.key === key)
        ?.state
    )
  }
}

async function setup(
  platform: ShortcutPlatform = { os: 'mac', build: 'desktop' },
  table: readonly ShortcutEntry[] = SHORTCUTS
) {
  const menu = new FakeMenu()
  const currentView = signal<ViewMode>('list')
  TestBed.configureTestingModule({
    providers: [
      provideRouter([
        { path: 'timeline', component: BlankComponent, data: { [SHORTCUT_SCOPE]: 'timeline' } },
        { path: 'day', component: BlankComponent },
      ]),
      { provide: SHORTCUT_PLATFORM, useValue: platform },
      { provide: NATIVE_MENU_IPC, useValue: menu },
      { provide: ActivityViewStateService, useValue: { currentView } },
      { provide: SHORTCUT_TABLE, useValue: table },
    ],
  })
  await TestBed.inject(Router).navigateByUrl('/timeline')
  const shortcuts = TestBed.inject(ShortcutService)
  const service = TestBed.inject(NativeMenuService)
  return { menu, shortcuts, service, currentView }
}

/** Installs the menu as generation `generation`. */
async function installed(menu: FakeMenu, shortcuts: ShortcutService, generation = 7) {
  await vi.waitFor(() => expect(menu.installs).toHaveLength(1))
  menu.installed.resolve({ kind: 'installed', generation })
  await vi.waitFor(() => expect(shortcuts.menuOwnsAccelerators()).toBe(true))
}

function pressCommandR(): KeyboardEvent {
  const event = new KeyboardEvent('keydown', {
    key: 'r',
    metaKey: true,
    bubbles: true,
    cancelable: true,
  })
  document.body.dispatchEvent(event)
  return event
}

describe('NativeMenuService', () => {
  afterEach(() => {
    document.body.replaceChildren()
  })

  it('leaves the menu its accelerators only once the desktop app has installed it', async () => {
    const { menu, shortcuts, service } = await setup()
    const collect = vi.fn()
    shortcuts.bind('app.collect', collect)

    service.start()
    await vi.waitFor(() => expect(menu.installs).toHaveLength(1))
    // Installing: the page still takes ⌘R.
    expect(pressCommandR().defaultPrevented).toBe(true)

    menu.installed.resolve({ kind: 'installed', generation: 7 })
    await vi.waitFor(() => expect(shortcuts.menuOwnsAccelerators()).toBe(true))
    expect(pressCommandR().defaultPrevented).toBe(false)
    expect(collect).toHaveBeenCalledOnce()
    expect(menu.shown('Collect')).toEqual({ kind: 'command', title: 'Collect', enabled: true })
    expect(menu.shown('Filter Activity…')).toMatchObject({ kind: 'command' })
    expect(menu.shown('Timeline')).toMatchObject({ kind: 'check', checked: true })
  })

  it('leaves every key to the page when the menu is not installed', async () => {
    const failed = await setup()
    failed.service.start()
    await vi.waitFor(() => expect(failed.menu.installs).toHaveLength(1))
    vi.spyOn(console, 'warn').mockImplementation(() => undefined)
    failed.menu.installed.reject(new BackendFailure('internal', 'no menu bar'))
    await vi.waitFor(() => expect(failed.menu.receive).toBeNull())
    expect(failed.shortcuts.menuOwnsAccelerators()).toBe(false)
    failed.shortcuts.bind('app.collect', vi.fn())
    expect(pressCommandR().defaultPrevented).toBe(true)
    TestBed.resetTestingModule()

    const notOffered = await setup()
    notOffered.service.start()
    await vi.waitFor(() => expect(notOffered.menu.installs).toHaveLength(1))
    notOffered.menu.installed.resolve({ kind: 'notOffered' })
    await vi.waitFor(() => expect(notOffered.menu.receive).toBeNull())
    expect(notOffered.shortcuts.menuOwnsAccelerators()).toBe(false)
  })

  it('builds no menu off macOS or in the browser', async () => {
    for (const platform of [
      { os: 'other', build: 'desktop' },
      { os: 'mac', build: 'browser' },
    ] as const) {
      const { menu, service } = await setup(platform)
      service.start()
      await Promise.resolve()
      expect(menu.installs).toHaveLength(0)
      TestBed.resetTestingModule()
    }
  })

  it('runs a chosen item as its key would, and only an item of the installed menu', async () => {
    const { menu, shortcuts, service } = await setup()
    const collect = vi.fn()
    const back = vi.fn()
    shortcuts.bind('app.collect', collect)
    shortcuts.bind('app.back', back)
    service.start()
    await vi.waitFor(() => expect(menu.installs).toHaveLength(1))

    // Chosen as the menu came in: it runs once the answer names its generation.
    menu.receive?.({ generation: 7, key: menu.key('Collect') })
    expect(collect).not.toHaveBeenCalled()
    await installed(menu, shortcuts)
    expect(collect).toHaveBeenCalledOnce()

    menu.receive?.({ generation: 6, key: menu.key('Collect') })
    menu.receive?.({ generation: 7 })
    menu.receive?.('collect')
    expect(collect).toHaveBeenCalledOnce()

    // Back from the menu does nothing while typing; Collect does.
    const input = document.createElement('input')
    document.body.append(input)
    input.focus()
    menu.receive?.({ generation: 7, key: menu.key('Back') })
    menu.receive?.({ generation: 7, key: menu.key('Collect') })
    expect(back).not.toHaveBeenCalled()
    expect(collect).toHaveBeenCalledTimes(2)
    input.blur()
    menu.receive?.({ generation: 7, key: menu.key('Back') })
    expect(back).toHaveBeenCalledOnce()
  })

  it('follows the app: the view on screen checked, an item off where its command would not run', async () => {
    const { menu, shortcuts, service, currentView } = await setup()
    const enabled = signal(true)
    shortcuts.bind('app.collect', vi.fn(), enabled)
    service.start()
    await installed(menu, shortcuts)
    TestBed.tick()
    expect(menu.updates).toHaveLength(0)

    currentView.set('day')
    TestBed.tick()
    await vi.waitFor(() => expect(menu.updates).toHaveLength(1))
    expect(menu.updates[0]?.generation).toBe(7)
    // Only what changed is sent.
    expect(menu.updates[0]?.items.map(item => item.state.title)).toEqual(['Timeline', 'Day'])
    expect(menu.shown('Day')).toMatchObject({ checked: true })
    expect(menu.shown('Timeline')).toMatchObject({ checked: false })

    const closePicker = shortcuts.openLayer({ kind: 'modal' })
    TestBed.tick()
    await vi.waitFor(() => expect(menu.shown('Collect')).toMatchObject({ enabled: false }))
    closePicker()
    TestBed.tick()
    await vi.waitFor(() => expect(menu.shown('Collect')).toMatchObject({ enabled: true }))
  })

  it('sends one update at a time, then the latest state', async () => {
    const { menu, shortcuts, service } = await setup()
    const enabled = signal(true)
    shortcuts.bind('app.collect', vi.fn(), enabled)
    service.start()
    await installed(menu, shortcuts)

    const first = deferred<void>()
    menu.nextUpdate = first
    enabled.set(false)
    TestBed.tick()
    await vi.waitFor(() => expect(menu.updates).toHaveLength(1))
    enabled.set(true)
    TestBed.tick()
    enabled.set(false)
    TestBed.tick()
    enabled.set(true)
    TestBed.tick()
    expect(menu.updates).toHaveLength(1)

    menu.nextUpdate = null
    first.resolve()
    await vi.waitFor(() => expect(menu.updates).toHaveLength(2))
    expect(menu.shown('Collect')).toMatchObject({ enabled: true })
    await Promise.resolve()
    expect(menu.updates).toHaveLength(2)
  })

  it('runs an item without a key while a field has focus: it is only ever clicked', async () => {
    const { menu, shortcuts, service } = await setup()
    const exportMarkdown = vi.fn()
    const help = vi.fn()
    shortcuts.bind('app.export-markdown', exportMarkdown)
    shortcuts.bind('app.help', help)
    service.start()
    await installed(menu, shortcuts)
    const input = document.createElement('input')
    document.body.append(input)
    input.focus()

    menu.receive?.({ generation: 7, key: menu.key('Export Markdown…') })
    menu.receive?.({ generation: 7, key: menu.key('Keyboard Shortcuts') })

    expect(exportMarkdown).toHaveBeenCalledOnce()
    expect(help).toHaveBeenCalledOnce()
  })

  it('shows an item off once the page shows its command off, as a disabled Collect button', async () => {
    const { menu, shortcuts, service } = await setup()
    const fixture = TestBed.createComponent(CollectButtonComponent)
    fixture.detectChanges()
    // The shell's own Collect, off while loading, as the button is.
    shortcuts.fallback('app.collect', vi.fn(), () => !fixture.componentInstance.loading())
    service.start()
    await installed(menu, shortcuts)

    fixture.componentInstance.loading.set(true)
    TestBed.tick()
    await vi.waitFor(() => expect(menu.shown('Collect')).toMatchObject({ enabled: false }))
    fixture.componentInstance.loading.set(false)
    TestBed.tick()
    await vi.waitFor(() => expect(menu.shown('Collect')).toMatchObject({ enabled: true }))
  })

  it('names Find after the entry ⌘F runs in the scope on screen, the last one in the end', async () => {
    // The Timeline with a ⌘F of its own beside the global one, as Work items will have.
    const timelineSearch: ShortcutEntry = {
      id: 'timeline.filter',
      label: 'Search Timeline…',
      scope: 'timeline',
      menu: 'edit',
      chords: [{ keys: 'mod+f', where: 'desktop', whileTyping: true, menuAccelerator: true }],
    }
    const table = SHORTCUTS.map(entry => (entry.id === 'timeline.filter' ? timelineSearch : entry))
    const { menu, shortcuts, service } = await setup(undefined, table)
    const router = TestBed.inject(Router)
    shortcuts.bind('timeline.filter', vi.fn())
    shortcuts.fallback('app.filter', vi.fn())
    service.start()
    await installed(menu, shortcuts)
    const find = menu.key('Search Timeline…')

    const first = deferred<void>()
    menu.nextUpdate = first
    await router.navigateByUrl('/day')
    TestBed.tick()
    await vi.waitFor(() => expect(menu.updates).toHaveLength(1))
    expect(menu.updates[0]?.items.find(item => item.key === find)?.state.title).toBe(
      'Filter Activity…'
    )
    // Further scope changes while that update is out: only the last is sent.
    await router.navigateByUrl('/timeline')
    TestBed.tick()
    await router.navigateByUrl('/day')
    TestBed.tick()
    await router.navigateByUrl('/timeline')
    TestBed.tick()
    menu.nextUpdate = null
    first.resolve()

    await vi.waitFor(() => expect(menu.updates).toHaveLength(2))
    expect(menu.shown('Search Timeline…')).toMatchObject({ title: 'Search Timeline…' })
  })
})
