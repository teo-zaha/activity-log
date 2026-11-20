import { DOCUMENT } from '@angular/common'
import { InjectionToken, Service, type Signal, computed, inject, signal } from '@angular/core'
import { MatDialog } from '@angular/material/dialog'
import { type ActivatedRouteSnapshot, Router } from '@angular/router'
import { listener, setupSync } from '@signality/core/browser/listener'
import {
  type Build,
  type Chord,
  SHORTCUT_SCOPES,
  SHORTCUTS,
  type ShortcutEntry,
  type ShortcutId,
  type ShortcutScope,
  chordsIn,
  resolveChord,
  shortcut,
} from '../models/shortcuts'
import { type Os, ariaChord, chordOf, formatChord, osOf } from '../utils/key-chord'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'

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

/** The route data key a view's route declares its scope under. */
export const SHORTCUT_SCOPE = 'shortcutScope'

/** The help's dialog, the one dialog under which a key still reaches the app (its own `?`). */
export const SHORTCUT_HELP_DIALOG_ID = 'shortcut-help'

/**
 * What lies over the route: a `page` (a detail page or panel, the phone's Settings tab), which
 * binds its own Escape and Back and, when `hidesRoute`, leaves only `global` entries; or a
 * `modal` that owns the keyboard while it is open, as a dialog does (an open date picker).
 */
export type ShortcutLayer =
  { readonly kind: 'page'; readonly hidesRoute: boolean } | { readonly kind: 'modal' }

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

/** Where a key types: a text field, a select or editable content. */
function isEditable(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false
  if (isContentEditable(target)) return true
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
  private readonly router = inject(Router)
  private readonly dialogs = inject(MatDialog)
  private readonly document = inject(DOCUMENT)
  private readonly bindings = new Map<ShortcutId, Binding[]>()
  private readonly fallbacks = new Map<ShortcutId, Binding>()
  private readonly layers = signal<readonly ShortcutLayer[]>([])

  private readonly routeScope = computed(() => {
    this.router.lastSuccessfulNavigation()
    return routeScopeOf(this.router.routerState.snapshot.root)
  })

  /** The scope whose entries apply besides the global ones. */
  readonly activeScope: Signal<ShortcutScope> = computed(() =>
    this.layers().some(layer => layer.kind === 'page' && layer.hidesRoute)
      ? 'global'
      : this.routeScope()
  )

  constructor() {
    // Bubbling, so a field or widget that handles a key first marks it handled. At once: a key
    // can come before the first render.
    setupSync(() => listener(this.document, 'keydown', event => this.onKeydown(event)))
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
    return () =>
      this.bindings.set(
        id,
        (this.bindings.get(id) ?? []).filter(b => b !== binding)
      )
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
    return () => {
      if (this.fallbacks.get(id) === binding) this.fallbacks.delete(id)
    }
  }

  /** Puts `layer` over the route until the returned function is called. */
  openLayer(layer: ShortcutLayer): () => void {
    this.layers.update(layers => [...layers, layer])
    return () => this.layers.update(layers => layers.filter(open => open !== layer))
  }

  /**
   * Runs `id` as its key would, from somewhere that is not a key press (a menu item): the scope,
   * the dialogs and the typing rule decide as for a key. An entry none of whose chords here fires
   * while typing waits while a field has focus.
   */
  trigger(id: ShortcutId): boolean {
    const entry = shortcut(id)
    const typing = isEditable(this.document.activeElement)
    if (typing && !this.chords(entry).some(chord => chord.whileTyping)) return false
    return this.handOff(id)
  }

  /**
   * Runs `id` for a command that hands on to it (⌘F to the Timeline's filter): the scope and the
   * dialogs decide; the typing rule does not, since the first command's key passed it already.
   */
  handOff(id: ShortcutId): boolean {
    const entry = shortcut(id)
    if (entry.scope !== 'global' && entry.scope !== this.activeScope()) return false
    return this.run(id)
  }

  /** The entry a key press of `keys` would run here and now, with the chord it matched. */
  private resolve(keys: string): { readonly entry: ShortcutEntry; readonly chord: Chord } | null {
    return resolveChord(SHORTCUTS, { ...this.platform, scope: this.activeScope() }, keys)
  }

  /** The chords of an entry here: on this platform, and in this build. */
  chords(entry: ShortcutEntry): readonly Chord[] {
    return chordsIn(entry, this.platform.os, this.platform.build)
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
    // Handled by a field or widget, mid-composition (IME), or held down.
    if (event.defaultPrevented || event.isComposing || event.keyCode === 229 || event.repeat) {
      return
    }
    const target = event.target
    if (target instanceof Element && target.closest('[inert]') !== null) return
    const keys = chordOf(event, this.platform.os)
    const match = keys === null ? null : this.resolve(keys)
    if (match === null) return
    if (isEditable(target) && !match.chord.whileTyping) return
    if (this.run(match.entry.id)) event.preventDefault()
  }

  /**
   * The top binding of `id` that takes it, else its fallback. A dialog or a modal layer owns its
   * keys, Escape included; the help takes only its own `?`, which closes it.
   */
  private run(id: ShortcutId): boolean {
    if (this.layers().some(layer => layer.kind === 'modal')) return false
    const open = this.dialogs.openDialogs
    if (open.length > 0) {
      const onlyHelp = open.every(dialog => dialog.id === SHORTCUT_HELP_DIALOG_ID)
      if (!onlyHelp || id !== 'app.help') return false
    }
    const stack = this.bindings.get(id) ?? []
    const fallback = this.fallbacks.get(id)
    const candidates = fallback ? [...stack].reverse().concat(fallback) : [...stack].reverse()
    return candidates.some(binding => binding.enabled() && binding.run() !== false)
  }
}
