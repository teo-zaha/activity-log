import { DOCUMENT } from '@angular/common'
import { InjectionToken, Service, type Signal, computed, inject, signal } from '@angular/core'
import { toSignal } from '@angular/core/rxjs-interop'
import { MatDialog } from '@angular/material/dialog'
import { type ActivatedRouteSnapshot, Router } from '@angular/router'
import { listener, setupSync } from '@signality/core/browser/listener'
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
      // Only to know when a held key is let go (`held`).
      listener(this.document, 'keyup', () => (this.held = false))
      const view = this.document.defaultView
      if (view) listener(view, 'blur', () => (this.held = false))
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
    if (match === null || (this.typing() && !firesWhileTyping(match.chord))) return false
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
    return this.run(id)
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

  private typing(): boolean {
    return isEditable(this.document.activeElement)
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
    if (isEditable(target) && !firesWhileTyping(match.chord)) return
    if (this.run(match.entry.id)) event.preventDefault()
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
