import { DOCUMENT } from '@angular/common'
import { InjectionToken, Service, type Signal, computed, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { MatDialog } from '@angular/material/dialog'
import { type ActivatedRouteSnapshot, Router } from '@angular/router'
import { listener, setupSync } from '@signality/core/browser/listener'
import { debounceCallback } from '@signality/core/scheduling/debounce-callback'
import { concat, map, mergeMap, of, scan } from 'rxjs'
import {
  type Build,
  type Chord,
  type ChordsHere,
  SHORTCUT_SCOPES,
  SHORTCUTS,
  type ShortcutEntry,
  type ShortcutId,
  type ShortcutScope,
  chordsIn,
  firesWhileTyping,
  isMenuAccelerator,
  resolveChord,
  shortcut,
} from '../models/shortcuts'
import {
  type CommandChordKeys,
  type Os,
  ariaChord,
  chordOf,
  formatChord,
  formatShift,
  osOf,
} from '../utils/key-chord'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'
import { ShortcutPreferences } from './shortcut-preferences.service'

export interface ShortcutPlatform {
  readonly os: Os
  readonly build: Build
}

/** Which keys apply: the platform's modifier, and whether the browser keeps its own keys. */
export const SHORTCUT_PLATFORM = new InjectionToken<ShortcutPlatform>('ShortcutPlatform', {
  providedIn: 'root',
  factory: (): ShortcutPlatform => ({
    os: osOf(inject(DOCUMENT).defaultView?.navigator.userAgent ?? ''),
    build: inject(RuntimeEnvironmentService).isTauri() ? 'desktop' : 'browser',
  }),
})

/**
 * The table keys and the macOS menu are resolved from: `SHORTCUTS`. A spec gives entries the app
 * has yet to add, such as a view's own ⌘F beside the global one.
 */
export const SHORTCUT_TABLE = new InjectionToken<readonly ShortcutEntry[]>('ShortcutTable', {
  providedIn: 'root',
  factory: (): readonly ShortcutEntry[] => SHORTCUTS,
})

/** The route data key a view's route declares its scope under. */
export const SHORTCUT_SCOPE = 'shortcutScope'

/** The help's dialog, the one dialog under which a key still reaches the app (its own `?`). */
export const SHORTCUT_HELP_DIALOG_ID = 'shortcut-help'

/**
 * What lies over the route: a `page` (the details, the phone's Settings tab), which binds its own
 * Escape and Back and, while it is on top, puts its `scope` on screen in place of the route's
 * (`global`: none besides the global entries); or a `modal` that owns the keyboard while it is
 * open, as a dialog does (an open date picker).
 */
export type ShortcutLayer =
  | {
      readonly kind: 'page'
      readonly scope: ShortcutScope
      /** Whether the page is the top one now (`TopLayer`), read as a signal. */
      readonly onTop: () => boolean
    }
  | { readonly kind: 'modal' }

type PageLayer = Extract<ShortcutLayer, { kind: 'page' }>

/**
 * How long a command may take to hand on to a field (`expectHandOff`) before the keys are the
 * app's again: a view's code may have to load first.
 */
export const HAND_OFF_LIMIT_MS = 3000

/** A command on its way to a field: what is typed meanwhile is meant for the field. */
type HandOff =
  | { readonly kind: 'none' }
  | { readonly kind: 'pending'; readonly id: ShortcutId; readonly until: number }

/**
 * How long Shift is held on its own before the key tags show (`revealKeys`): long enough that a
 * Shift on its way to a letter, Tab, a click or the wheel rarely flashes them.
 */
export const REVEAL_DELAY_MS = 400

/** Shift held on its own: `pressed` until `REVEAL_DELAY_MS` passed, then `held`. */
type ShiftHold = 'none' | 'pressed' | 'held'

interface Binding {
  readonly run: () => void | false
  readonly enabled: () => boolean
}

const TEXT_INPUT_TYPES = new Set([
  'date',
  'datetime-local',
  'email',
  'month',
  'number',
  'password',
  'search',
  'tel',
  'text',
  'time',
  'url',
  'week',
])

/**
 * Whether `element` is editable content: the browser's own answer, and where it gives none
 * (jsdom), the nearest element that says (`contenteditable="false"` ends an editor's content).
 */
function isContentEditable(element: HTMLElement): boolean {
  if (typeof element.isContentEditable === 'boolean') return element.isContentEditable
  const boundary = element.closest('[contenteditable]')
  return boundary !== null && boundary.getAttribute('contenteditable') !== 'false'
}

/**
 * Widgets whose letters pick an item (typeahead): a Material select's host is a `combobox`, and
 * its open list a `listbox`. They let the key through, so a letter there is typing.
 */
const TYPEAHEAD =
  '[role="combobox"], [role="listbox"], [role="menu"], [role="menubar"], [role="tree"], [role="treegrid"], [role="grid"]'

/** Where a key types: a text field, a select, a typeahead widget or editable content. */
function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (isContentEditable(target)) return true
  if (target.closest(TYPEAHEAD) !== null) return true
  if (target instanceof HTMLTextAreaElement || target instanceof HTMLSelectElement) return true
  return target instanceof HTMLInputElement && TEXT_INPUT_TYPES.has(target.type)
}

/** One `aria-keyshortcuts` for a control that several shortcuts work: each one's keys here. */
export function ariaKeysOf(keys: readonly (string | null)[]): string | null {
  const named = keys.filter(key => key !== null)
  return named.length > 0 ? named.join(' ') : null
}

function isShortcutScope(value: unknown): value is ShortcutScope {
  return SHORTCUT_SCOPES.some(scope => scope === value)
}

/** The scope of the deepest route on screen that declares one; `global` without one. */
function routeScopeOf(root: ActivatedRouteSnapshot): ShortcutScope {
  let scope: ShortcutScope = 'global'
  for (let route: ActivatedRouteSnapshot | null = root; route; route = route.firstChild) {
    const declared: unknown = route.data[SHORTCUT_SCOPE]
    if (isShortcutScope(declared)) scope = declared
  }
  return scope
}

/**
 * The app's shortcuts: the one key listener for them, and the rule that decides which entry of
 * `models/shortcuts.ts` a key press runs. Whoever owns a command binds it (`bind`, or the
 * `[appShortcut]` directive on its control); the app's own commands are fallbacks for when no
 * control takes the key (`fallback`). Keys on a widget's own element (lists, the day index, the
 * segmented control) stay with the widget.
 */
@Service()
export class ShortcutService {
  private readonly platform = inject(SHORTCUT_PLATFORM)
  private readonly preferences = inject(ShortcutPreferences)
  /** Which chords work here: the platform's, and character keys while the setting has them on. */
  private readonly here = computed<ChordsHere>(() => ({
    ...this.platform,
    characterKeys: this.preferences.characterKeys(),
  }))
  private readonly table = inject(SHORTCUT_TABLE)
  private readonly router = inject(Router)
  private readonly dialogs = inject(MatDialog)
  private readonly document = inject(DOCUMENT)
  private readonly bindings = new Map<ShortcutId, Binding[]>()
  private readonly fallbacks = new Map<ShortcutId, Binding>()
  /** Raised as bindings come and go, so what reads them (`available`) reads them again. */
  private readonly bound = signal(0)
  private readonly layers = signal<readonly ShortcutLayer[]>([])
  /** The ids of the open dialogs, as each opens and until it has closed. */
  private readonly openDialogs = toSignal(
    this.dialogs.afterOpened.pipe(
      mergeMap(dialog =>
        concat(
          of({ dialog, open: true }),
          dialog.afterClosed().pipe(map(() => ({ dialog, open: false })))
        )
      ),
      scan(
        (open: readonly string[], change) =>
          change.open ? [...open, change.dialog.id] : open.filter(id => id !== change.dialog.id),
        this.dialogs.openDialogs.map(dialog => dialog.id)
      )
    ),
    { initialValue: this.dialogs.openDialogs.map(dialog => dialog.id) }
  )

  /**
   * Set by the macOS menu once it is installed: from then on it takes the chords that are its
   * accelerators (`menuAccelerator`) and runs them through `triggerChord`, so the page leaves
   * them alone. Before that, after a failed installation and everywhere else the page takes
   * every chord.
   */
  readonly menuOwnsAccelerators = signal(false)

  /**
   * A key is held down: the latest press was an auto-repeat and no key went up since. WebKit
   * hands each repeat of a ⌘ key the page leaves on to the menu, also one the page cannot read as
   * its chord (macOS shows ⌘[ as ⌘Ö on a German layout), so the menu's commands wait it out.
   */
  private held = false

  /** A hand-off to a field under way (`expectHandOff`), until `handOff` makes it. */
  private handOffState: HandOff = { kind: 'none' }

  private readonly shiftHold = signal<ShiftHold>('none')
  /** Each Shift press starts it again; it reveals only if Shift is still held on its own. */
  private readonly revealAfterDelay = debounceCallback(() => {
    if (this.shiftHold() === 'pressed') this.shiftHold.set('held')
  }, REVEAL_DELAY_MS)

  /**
   * Whether the key tags show: Shift is held on its own, past `REVEAL_DELAY_MS`, and no dialog
   * or modal layer holds the keys (the help included, whose `?` is the only key that works).
   */
  readonly revealKeys: Signal<boolean> = computed(
    () =>
      this.shiftHold() === 'held' &&
      this.openDialogs().length === 0 &&
      !this.layers().some(layer => layer.kind === 'modal')
  )

  private readonly routeScope = computed(() => {
    this.router.lastSuccessfulNavigation()
    return routeScopeOf(this.router.routerState.snapshot.root)
  })

  /**
   * The scope whose entries apply besides the global ones: the page's on top, else the latest
   * page's (under a dialog), else the route's. `TopLayer` decides which page is on top, not the
   * order they opened in: the phone's Settings tab may open under open details.
   */
  readonly activeScope: Signal<ShortcutScope> = computed(() => {
    const pages = this.layers().filter((layer): layer is PageLayer => layer.kind === 'page')
    const page = pages.find(layer => layer.onTop()) ?? pages.at(-1)
    return page?.scope ?? this.routeScope()
  })

  constructor() {
    // Bubbling, so a field or widget that handles a key first marks it handled. At once: a key
    // can come before the first render.
    setupSync(() => {
      listener(this.document, 'keydown', event => this.onKeydown(event))
      // To know when a held key is let go (`held`, `shiftHold`).
      listener(this.document, 'keyup', event => {
        this.held = false
        if (event.key === 'Shift') this.shiftHold.set('none')
      })
      // Shift+click is no reveal; captured, so a control that stops the event still ends it.
      listener.capture(this.document, 'pointerdown', () => this.shiftHold.set('none'))
      // A pointer that moves without Shift says it is up, also when its `keyup` was lost.
      listener.capture(this.document, 'pointermove', event => {
        if (!event.shiftKey) this.shiftHold.set('none')
      })
      // A key let go elsewhere never comes back as `keyup`.
      listener(this.document, 'visibilitychange', () => this.shiftHold.set('none'))
      const view = this.document.defaultView
      if (view)
        listener(view, 'blur', () => {
          this.held = false
          this.shiftHold.set('none')
        })
    })
  }

  /**
   * Runs `run` for `id` until the returned function is called. The latest binding of an entry is
   * tried first; one that is not `enabled`, or whose `run` returns false, leaves it to the one
   * before, then to the entry's fallback.
   */
  bind(id: ShortcutId, run: () => void | false, enabled: () => boolean = () => true): () => void {
    const binding: Binding = { run, enabled }
    const stack = this.bindings.get(id) ?? []
    this.bindings.set(id, [...stack, binding])
    this.bound.update(count => count + 1)
    return () => {
      this.bindings.set(
        id,
        (this.bindings.get(id) ?? []).filter(b => b !== binding)
      )
      this.bound.update(count => count + 1)
    }
  }

  /** The app's own command for `id`, tried once no binding takes the key. One per entry. */
  fallback(
    id: ShortcutId,
    run: () => void | false,
    enabled: () => boolean = () => true
  ): () => void {
    if (this.fallbacks.has(id)) throw new Error(`${id} has a fallback already`)
    const binding: Binding = { run, enabled }
    this.fallbacks.set(id, binding)
    this.bound.update(count => count + 1)
    return () => {
      if (this.fallbacks.get(id) !== binding) return
      this.fallbacks.delete(id)
      this.bound.update(count => count + 1)
    }
  }

  /** Puts `layer` over the route until the returned function is called. */
  openLayer(layer: ShortcutLayer): () => void {
    this.layers.update(layers => [...layers, layer])
    return () => this.layers.update(layers => layers.filter(open => open !== layer))
  }

  /**
   * Runs what a press of `keys` would run here and now, from somewhere that is not a key press
   * (a menu item's accelerator, which macOS takes before the page): the scope, the dialogs and
   * the typing rule of the chord it matches decide as for a key, and nothing runs while a key is
   * held (`held`).
   */
  triggerChord(keys: CommandChordKeys): boolean {
    // A held accelerator runs once, as a held key does.
    if (this.held) return false
    const match = this.match(keys)
    const typing = this.typingAt(this.document.activeElement)
    if (match === null || (typing && !firesWhileTyping(match.chord))) return false
    return this.run(match.entry.id)
  }

  /**
   * Runs `id` for a menu item without an accelerator, which only a click chooses: the scope, the
   * dialogs and the bindings decide; the typing rule does not, since no key was pressed.
   */
  choose(id: ShortcutId): boolean {
    return this.run(id)
  }

  /**
   * Runs `id` for a command that hands on to it (⌘F to the Timeline's filter): the scope and the
   * dialogs decide; the typing rule does not, since the first command's key passed it already.
   */
  handOff(id: ShortcutId): boolean {
    if (this.handOffState.kind === 'pending' && this.handOffState.id === id) {
      this.handOffState = { kind: 'none' }
    }
    return this.run(id)
  }

  /**
   * Says a command is on its way to the field `id` stands for (`/` from a calendar view switches
   * to the Timeline first): until `handOff(id)`, the returned function or `HAND_OFF_LIMIT_MS`,
   * keys count as typing, so the filter's first letters run no shortcut.
   */
  expectHandOff(id: ShortcutId): () => void {
    const pending: HandOff = { kind: 'pending', id, until: performance.now() + HAND_OFF_LIMIT_MS }
    this.handOffState = pending
    return () => {
      if (this.handOffState === pending) this.handOffState = { kind: 'none' }
    }
  }

  /** The entry a press of `keys` runs here, following the scope on screen (a menu item's title). */
  resolve(keys: CommandChordKeys): Signal<ShortcutEntry | null> {
    return computed(() => this.match(keys)?.entry ?? null)
  }

  /**
   * Whether `id` would run now, from a key or a menu item, as far as signals tell: its scope is
   * on screen, no modal layer or dialog holds the keys, and a binding or fallback is enabled. The
   * typing rule is not in it (focus is no signal); `triggerChord` applies it when an item with an
   * accelerator is chosen.
   */
  available(id: ShortcutId): boolean {
    return this.takers(id).length > 0
  }

  /** The entry a key press of `keys` would run here and now, with the chord it matched. */
  private match(keys: string): { readonly entry: ShortcutEntry; readonly chord: Chord } | null {
    return resolveChord(this.table, { ...this.here(), scope: this.activeScope() }, keys)
  }

  /** Whether a key at `target` types: in an editable place, or on its way to a field. */
  private typingAt(target: EventTarget | null): boolean {
    return isEditable(target) || this.handingOff()
  }

  private handingOff(): boolean {
    const state = this.handOffState
    if (state.kind === 'none') return false
    if (performance.now() < state.until) return true
    this.handOffState = { kind: 'none' }
    return false
  }

  /** Whether single-key shortcuts are on (the help says so when they are not). */
  readonly characterKeys = this.preferences.characterKeys

  /** The chords of an entry here: on this platform, in this build, with the single-key setting. */
  chords(entry: ShortcutEntry): readonly Chord[] {
    return chordsIn(entry, this.here())
  }

  /** The key tag of `id`, as its first chord here reads (`⌘R`); null where it has none. */
  hint(id: ShortcutId): string | null {
    const chord = this.chords(shortcut(id))[0]
    return chord ? formatChord(chord.keys, this.platform.os) : null
  }

  /** How Shift reads here, on the tag that says holding it shows the others. */
  readonly shiftKey = formatShift(this.platform.os)

  /**
   * Whether holding Shift can show a key tag here: some entry has a key on this platform, in this
   * build and with the single-key setting. Escape (`layer.close`), the one key left everywhere,
   * closes only what is open and is alone no reason to hold Shift.
   */
  readonly revealable: Signal<boolean> = computed(() =>
    this.table.some(entry => entry.id !== 'layer.close' && this.chords(entry).length > 0)
  )

  /** How people read `chord` here. */
  format(chord: Chord): string {
    return formatChord(chord.keys, this.platform.os)
  }

  /** `aria-keyshortcuts` for `id` here: each of its chords; null where it has none. */
  ariaKeys(id: ShortcutId): string | null {
    const chords = this.chords(shortcut(id))
    return chords.length > 0
      ? chords.map(chord => ariaChord(chord.keys, this.platform.os)).join(' ')
      : null
  }

  private onKeydown(event: KeyboardEvent): void {
    this.followShift(event)
    // Widgets claim only unmodified keys, so a held ⌘ key always gets here.
    this.held = event.repeat
    // Handled by a field or widget, mid-composition (IME), or held down.
    if (event.defaultPrevented || event.isComposing || event.keyCode === 229 || event.repeat) {
      return
    }
    const target = event.target
    if (target instanceof Element && target.closest('[inert]') !== null) return
    const keys = chordOf(event, this.platform.os)
    const match = keys === null ? null : this.match(keys)
    if (match === null || this.menuTakes(match.chord)) return
    if (this.typingAt(target) && !firesWhileTyping(match.chord)) return
    if (this.run(match.entry.id)) event.preventDefault()
  }

  /**
   * Shift pressed on its own, away from fields and typeahead widgets and outside `inert`, starts
   * the reveal; its auto-repeat leaves it as it is, and any other key ends it (Shift+letter types
   * or is a chord), also one a field or widget handled.
   */
  private followShift(event: KeyboardEvent): void {
    if (event.key !== 'Shift') {
      this.shiftHold.set('none')
      return
    }
    if (event.repeat) return
    const target = event.target
    const alone = !event.ctrlKey && !event.altKey && !event.metaKey
    const free = !(target instanceof Element && target.closest('[inert]') !== null)
    if (alone && free && !this.typingAt(target)) {
      this.shiftHold.set('pressed')
      this.revealAfterDelay()
    } else {
      this.shiftHold.set('none')
    }
  }

  /** Whether the installed macOS menu takes `chord` before the page (`menuOwnsAccelerators`). */
  private menuTakes(chord: Chord): boolean {
    return (
      isMenuAccelerator(chord) &&
      this.platform.os === 'mac' &&
      this.platform.build === 'desktop' &&
      this.menuOwnsAccelerators()
    )
  }

  /** The first binding of `id` that runs it, else its fallback; whether one did. */
  private run(id: ShortcutId): boolean {
    return this.takers(id).some(binding => binding.run() !== false)
  }

  /**
   * The bindings that would take `id` now, latest first, then its fallback: none while its scope
   * is not on screen or a dialog or a modal layer owns the keys, Escape included; the help takes
   * only its own `?`, which closes it. The one rule for keys, menu items and `available`.
   */
  private takers(id: ShortcutId): readonly Binding[] {
    const entry = shortcut(id)
    if (entry.scope !== 'global' && entry.scope !== this.activeScope()) return []
    if (this.layers().some(layer => layer.kind === 'modal')) return []
    const open = this.openDialogs()
    if (open.length > 0) {
      const onlyHelp = open.every(dialog => dialog === SHORTCUT_HELP_DIALOG_ID)
      if (!onlyHelp || id !== 'app.help') return []
    }
    this.bound()
    const stack = this.bindings.get(id) ?? []
    const fallback = this.fallbacks.get(id)
    const candidates = fallback ? [...stack].reverse().concat(fallback) : [...stack].reverse()
    return candidates.filter(binding => binding.enabled())
  }
}
