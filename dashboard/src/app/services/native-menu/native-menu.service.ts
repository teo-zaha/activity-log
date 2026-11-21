import {
  type AfterRenderRef,
  DestroyRef,
  InjectionToken,
  Injector,
  Service,
  type Signal,
  afterRenderEffect,
  computed,
  inject,
  untracked,
} from '@angular/core'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import {
  MENU_COMMANDS,
  MENU_COMMAND_EVENT,
  type MenuCommand,
  type MenuInstall,
  type MenuItemKey,
  type MenuItemState,
  type MenuItemUpdate,
  type MenuSpec,
} from '../../generated/activity'
import type { ShortcutEntry } from '../../models/shortcuts'
import { ActivityViewStateService } from '../activity-view-state.service'
import { tauriFailureOf } from '../backend/backend-failure'
import { failureWhile } from '../failure'
import { SHORTCUT_PLATFORM, SHORTCUT_TABLE, ShortcutService } from '../shortcut.service'
import { type MenuItemPlan, menuPlanOf, menuSpecOf, sameState } from './menu-plan'

/** The desktop app's menu commands and event (`desktop/src/app_menu/`). */
export interface NativeMenuIpc {
  install(spec: MenuSpec): Promise<MenuInstall>
  update(generation: number, items: readonly MenuItemUpdate[]): Promise<void>
  /** Hands each `menu-command` payload, unchecked, to `receive` once it listens. */
  listen(receive: (payload: unknown) => void): Promise<() => void>
}

const TAURI_MENU_IPC: NativeMenuIpc = {
  install: spec =>
    invoke<MenuInstall>(MENU_COMMANDS.install, { spec }).catch((error: unknown) => {
      throw tauriFailureOf(error)
    }),
  update: (generation, items) =>
    invoke<null>(MENU_COMMANDS.update, { generation, items }).then(
      () => undefined,
      (error: unknown) => {
        throw tauriFailureOf(error)
      }
    ),
  listen: receive =>
    listen<unknown>(MENU_COMMAND_EVENT, event => receive(event.payload)).catch((error: unknown) => {
      throw tauriFailureOf(error)
    }),
}

export const NATIVE_MENU_IPC = new InjectionToken<NativeMenuIpc>('NativeMenuIpc', {
  providedIn: 'root',
  factory: (): NativeMenuIpc => TAURI_MENU_IPC,
})

function isMenuCommand(payload: unknown): payload is MenuCommand {
  if (typeof payload !== 'object' || payload === null) return false
  const generation: unknown = Reflect.get(payload, 'generation')
  const key: unknown = Reflect.get(payload, 'key')
  return Number.isInteger(generation) && Number.isInteger(key)
}

/**
 * Where the menu is: not started (or not on macOS), installing (clicks that arrive before its
 * answer wait), installed under a generation with what its items were last sent, or off: not
 * offered, or it failed, so the page keeps every key.
 */
type Phase =
  | { readonly kind: 'idle' }
  | { readonly kind: 'installing'; readonly held: MenuCommand[] }
  | {
      readonly kind: 'installed'
      readonly generation: number
      readonly sent: Map<MenuItemKey, MenuItemState>
    }
  | { readonly kind: 'off' }

/**
 * The macOS menu bar, made from the shortcut table (`menuPlanOf`) and installed by the desktop
 * app. Its items follow the app: a view is checked while it is on screen, a shared accelerator's
 * item is named after the entry it resolves to in the scope on screen (⌘F), and an item is
 * disabled where its command would not run. A chosen item with an accelerator runs as its key
 * would (`triggerChord`), one without as a click (`choose`). Once the menu is installed it owns its accelerators, and the page leaves them
 * to it (`menuOwnsAccelerators`). Its operations go one after another; updates send what changed
 * since the last one, the latest state only.
 */
@Service()
export class NativeMenuService {
  private readonly shortcuts = inject(ShortcutService)
  private readonly viewState = inject(ActivityViewStateService)
  private readonly platform = inject(SHORTCUT_PLATFORM)
  private readonly table = inject(SHORTCUT_TABLE)
  private readonly ipc = inject(NATIVE_MENU_IPC)
  private readonly injector = inject(Injector)
  private phase: Phase = { kind: 'idle' }
  private items = new Map<MenuItemKey, MenuItemPlan>()
  private latest: readonly MenuItemUpdate[] = []
  private flushing = false
  private unlisten: (() => void) | null = null
  private following: AfterRenderRef | null = null

  constructor() {
    inject(DestroyRef).onDestroy(() => this.unlisten?.())
  }

  /** Builds and installs the menu, in the desktop app on macOS only, once. */
  start(): void {
    if (this.phase.kind !== 'idle') return
    if (this.platform.os !== 'mac' || this.platform.build !== 'desktop') {
      this.phase = { kind: 'off' }
      return
    }
    let plan
    try {
      plan = menuPlanOf(this.table, this.platform.os)
    } catch (error) {
      this.fail(error)
      return
    }
    const items = plan.flatMap(section => section.items)
    this.items = new Map(items.map(item => [item.key, item]))
    const states = this.statesOf(items)
    this.phase = { kind: 'installing', held: [] }
    // After each render, so what a binding reads from the page (a disabled button) is as shown.
    this.following = afterRenderEffect(
      () => {
        this.latest = states()
        this.flush()
      },
      { injector: this.injector }
    )
    void this.install(states, state => menuSpecOf(plan, state))
  }

  private async install(
    states: Signal<readonly MenuItemUpdate[]>,
    specOf: (stateOf: (item: MenuItemPlan) => MenuItemState) => MenuSpec
  ): Promise<void> {
    try {
      // Listening first, so a choice made as soon as the menu shows is not missed.
      this.unlisten = await this.ipc.listen(payload => this.receive(payload))
      const initial = new Map(untracked(states).map(update => [update.key, update.state]))
      const shown = (item: MenuItemPlan): MenuItemState => {
        const state = initial.get(item.key)
        if (state === undefined) throw new Error(`No state for menu item ${item.key}`)
        return state
      }
      const answer = await this.ipc.install(specOf(shown))
      if (answer.kind === 'notOffered') {
        this.stop()
        return
      }
      const held = this.phase.kind === 'installing' ? this.phase.held : []
      this.phase = { kind: 'installed', generation: answer.generation, sent: initial }
      this.shortcuts.menuOwnsAccelerators.set(true)
      for (const command of held) this.receive(command)
      this.flush()
    } catch (error) {
      this.fail(error)
    }
  }

  /** What each item shows now, following the scope, the view and what can run. */
  private statesOf(items: readonly MenuItemPlan[]): Signal<readonly MenuItemUpdate[]> {
    const resolved = items.map(item => {
      const action = item.action
      const entry: Signal<ShortcutEntry | null> =
        action.kind === 'chord'
          ? this.shortcuts.resolve(action.keys)
          : computed(() => item.entries[0])
      return { item, entry }
    })
    return computed(() =>
      resolved.map(({ item, entry }): MenuItemUpdate => {
        const runs = entry()
        const title = (runs ?? item.entries[0]).label
        const enabled = runs !== null && this.shortcuts.available(runs.id)
        const view = item.view
        return {
          key: item.key,
          state:
            view === null
              ? { kind: 'command', title, enabled }
              : { kind: 'check', title, enabled, checked: this.viewState.currentView() === view },
        }
      })
    )
  }

  /**
   * Sends what changed since the last update the menu took, once the one before is answered. A
   * failed update is sent again with the next change, not at once.
   */
  private flush(): void {
    if (this.phase.kind !== 'installed' || this.flushing) return
    const { generation, sent } = this.phase
    const changed = this.latest.filter(update => {
      const before = sent.get(update.key)
      return before === undefined || !sameState(before, update.state)
    })
    if (changed.length === 0) return
    this.flushing = true
    void this.ipc.update(generation, changed).then(
      () => {
        for (const update of changed) sent.set(update.key, update.state)
        this.flushing = false
        this.flush()
      },
      (error: unknown) => {
        this.flushing = false
        const failure = failureWhile('Updating the macOS menu', error)
        console.warn(failure.message, failure.cause)
      }
    )
  }

  /** A chosen item of the installed menu runs as its key would; one of another menu does not. */
  private receive(payload: unknown): void {
    if (!isMenuCommand(payload)) return
    switch (this.phase.kind) {
      case 'installing':
        this.phase.held.push(payload)
        return
      case 'installed': {
        if (payload.generation !== this.phase.generation) return
        const item = this.items.get(payload.key)
        if (item === undefined) return
        if (item.action.kind === 'chord') this.shortcuts.triggerChord(item.action.keys)
        else this.shortcuts.choose(item.action.id)
        return
      }
      case 'idle':
      case 'off':
        return
    }
  }

  private fail(error: unknown): void {
    const failure = failureWhile('Installing the macOS menu', error)
    console.warn(failure.message, failure.cause)
    this.stop()
  }

  /** The page keeps every key. */
  private stop(): void {
    this.phase = { kind: 'off' }
    this.shortcuts.menuOwnsAccelerators.set(false)
    this.unlisten?.()
    this.unlisten = null
    this.following?.destroy()
    this.following = null
  }
}
