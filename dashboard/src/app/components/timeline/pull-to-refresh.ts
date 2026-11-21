import { type ElementRef, type Signal, computed, effect, signal, untracked } from '@angular/core'
import { listener } from '@signality/core/browser/listener'

/** How far the indicator has come down, past which releasing collects. */
export const PULL_THRESHOLD_PX = 64
/** How far the indicator comes down at most, however far the finger goes. */
export const PULL_LIMIT_PX = 160
/**
 * A touch that starts this close to the screen's side edge is left to the system: iOS's edge
 * swipe goes back, and a pull is never what such a touch means.
 */
export const PULL_EDGE_PX = 24

/**
 * Where the indicator is for a finger `travel` pixels below where it pressed: it follows, ever
 * more slowly, up to `PULL_LIMIT_PX`.
 */
export function resisted(travel: number): number {
  const down = Math.max(0, travel)
  return (PULL_LIMIT_PX * down) / (down + PULL_LIMIT_PX)
}

/** A finger on the list that is ours to follow. */
interface Held {
  /** The touch's `identifier`. */
  readonly touch: number
  /** Where it pressed, in viewport pixels. */
  readonly y: number
}

/** Where a pull on the Timeline stands. */
export type PullState =
  | { readonly kind: 'idle' }
  /** A finger pressed at the top of the list: its first move tells a pull from a scroll. */
  | (Held & { readonly kind: 'pressed'; readonly x: number })
  /** The indicator follows the finger, short of the threshold: releasing collects nothing. */
  | (Held & {
      readonly kind: 'pulling'
      readonly distance: number
      /** Whether this pull was past the threshold before, so that is said once. */
      readonly reached: boolean
    })
  /** Past the threshold: releasing collects, and the indicator goes back up at once. */
  | (Held & { readonly kind: 'armed'; readonly distance: number })

export type PullInput =
  | { readonly kind: 'press'; readonly touch: number; readonly x: number; readonly y: number }
  | { readonly kind: 'move'; readonly touch: number; readonly x: number; readonly y: number }
  | { readonly kind: 'release'; readonly touch: number }
  /** The touch was cancelled, a second finger came, or pulling is no longer offered. */
  | { readonly kind: 'cancel' }

/** What a step asks of the page beside its new state. */
export type PullEffect = 'none' | 'announce' | 'refresh'

export interface PullStep {
  readonly state: PullState
  readonly effect: PullEffect
}

const IDLE: PullState = { kind: 'idle' }
const stay = (state: PullState): PullStep => ({ state, effect: 'none' })

/** The pull a finger at `y` makes of a hold, past or short of the threshold. */
function follow(held: Held, y: number, reached: boolean): PullStep {
  const distance = resisted(y - held.y)
  const base = { touch: held.touch, y: held.y, distance }
  if (distance >= PULL_THRESHOLD_PX) {
    return { state: { ...base, kind: 'armed' }, effect: reached ? 'none' : 'announce' }
  }
  return stay({ ...base, kind: 'pulling', reached })
}

/**
 * The next state of a pull. A press only counts where a pull may start (`pullMayStart`); its
 * first move is a pull if it goes down at least as much as sideways, and otherwise leaves the
 * touch to the browser (a scroll, the system's edge swipe). A release past the threshold asks
 * for the collection and is over: what it collects shows as every collection does.
 */
export function stepPull(state: PullState, input: PullInput): PullStep {
  switch (input.kind) {
    case 'press':
      // A second finger: a pinch or a scroll, never a pull.
      if (state.kind !== 'idle') return stay(IDLE)
      return stay({ kind: 'pressed', touch: input.touch, x: input.x, y: input.y })
    case 'move': {
      if (state.kind === 'idle') return stay(state)
      if (input.touch !== state.touch) return stay(state)
      if (state.kind === 'pressed') {
        const down = input.y - state.y
        const across = Math.abs(input.x - state.x)
        if (down === 0 && across === 0) return stay(state)
        return down > 0 && down >= across ? follow(state, input.y, false) : stay(IDLE)
      }
      return follow(state, input.y, state.kind === 'armed' || state.reached)
    }
    case 'release':
      if (state.kind === 'idle') return stay(state)
      if (input.touch !== state.touch) return stay(state)
      return { state: IDLE, effect: state.kind === 'armed' ? 'refresh' : 'none' }
    case 'cancel':
      return stay(IDLE)
  }
}

/** How far down the indicator shows: with the finger while it pulls, and otherwise out of sight. */
export function indicatorDistance(state: PullState): number {
  switch (state.kind) {
    case 'pulling':
    case 'armed':
      return state.distance
    case 'idle':
    case 'pressed':
      return 0
  }
}

/** Whether the finger pulls: the touch is the page's, not the browser's to scroll with. */
export function isPulling(
  state: PullState
): state is Extract<PullState, { kind: 'pulling' | 'armed' }> {
  return state.kind === 'pulling' || state.kind === 'armed'
}

/** Where a touch pressed, and what was under it. */
export interface PressPlace {
  /** The list's scroll position; below zero while iOS bounces it at the top. */
  readonly scrollTop: number
  /** The finger's distance from the screen's left edge, and the screen's width. */
  readonly x: number
  readonly width: number
  /** Whether it pressed on the filter or the day index, which keep their own touches. */
  readonly onControls: boolean
}

/** Whether a press may become a pull: at the top of the list, on the list, away from the edges. */
export function pullMayStart(place: PressPlace): boolean {
  return (
    place.scrollTop <= 0 &&
    !place.onControls &&
    place.x >= PULL_EDGE_PX &&
    place.x <= place.width - PULL_EDGE_PX
  )
}

export interface PullToRefreshOptions {
  readonly scroller: Signal<ElementRef<HTMLElement>>
  /** The controls at the top of the list (the filter, the day index): no pull starts on them. */
  readonly controls: Signal<ElementRef<HTMLElement>>
  /** Whether a pull may start: the Timeline on screen with nothing over it, idle, with sources. */
  readonly enabled: Signal<boolean>
  /** Collects again. */
  readonly refresh: () => void
  /** Says, once per pull, that releasing now collects. */
  readonly announce: () => void
}

/**
 * Pull to refresh on a scroller, for fingers only: touch events, which neither a mouse, a
 * trackpad nor a wheel sends. A pen pressing on an iPad sends them too, and is left out.
 *
 * Hand-written on Signality's `listener`: its `swipe` listens passively, so it cannot keep the
 * list from scrolling or bouncing while the finger pulls. Only the indicator moves; the list keeps
 * its scroll position throughout, so neither its restore nor its anchoring sees anything.
 */
export function pullToRefresh(options: PullToRefreshOptions): Signal<PullState> {
  const { scroller } = options
  const state = signal<PullState>(IDLE)
  const atTop = signal(true)

  const apply = (input: PullInput): PullStep => {
    const step = stepPull(state(), input)
    state.set(step.state)
    if (step.effect === 'announce') options.announce()
    if (step.effect === 'refresh') options.refresh()
    return step
  }

  listener.passive(scroller, 'scroll', () => atTop.set(scroller().nativeElement.scrollTop <= 0))

  listener.passive(scroller, 'touchstart', (event: TouchEvent) => {
    const touch = event.changedTouches[0]
    if (event.touches.length > 1 || !touch) {
      apply({ kind: 'cancel' })
      return
    }
    const element = scroller().nativeElement
    const target = event.target
    const may =
      untracked(options.enabled) &&
      !isStylus(touch) &&
      pullMayStart({
        scrollTop: element.scrollTop,
        x: touch.clientX,
        width: element.ownerDocument.documentElement.clientWidth,
        onControls:
          target instanceof Node && untracked(options.controls).nativeElement.contains(target),
      })
    if (may) apply({ kind: 'press', touch: touch.identifier, x: touch.clientX, y: touch.clientY })
  })

  // Not passive, so a pull keeps the list from scrolling and from bouncing. Only listened for at
  // the top of the list, where a pull can start: elsewhere the browser scrolls without asking the
  // page first.
  const movesHere = computed(() => (atTop() && options.enabled() ? scroller() : null))
  /**
   * The finger a pull follows, among the touches of `event`; null for another touch. A pull no
   * longer offered is dropped first: the effect below does that too, but a Timeline the router
   * holds out of the document runs no effects, and the touch's events still reach its list.
   */
  const followed = (event: TouchEvent): Touch | null => {
    const before = state()
    if (before.kind !== 'pressed' && !isPulling(before)) return null
    if (!untracked(options.enabled)) {
      apply({ kind: 'cancel' })
      return null
    }
    return touchOf(event, before.touch)
  }

  listener(movesHere, 'touchmove', (event: TouchEvent) => {
    const touch = followed(event)
    if (!touch) return
    // The browser scrolls already: it took the touch before the page could.
    if (!event.cancelable) {
      apply({ kind: 'cancel' })
      return
    }
    const step = apply({
      kind: 'move',
      touch: touch.identifier,
      x: touch.clientX,
      y: touch.clientY,
    })
    if (isPulling(step.state)) event.preventDefault()
  })

  // A pull that ends on an entry does not click it: the finger moved, so the browser sends none.
  listener.passive(scroller, 'touchend', (event: TouchEvent) => {
    const touch = followed(event)
    if (touch) apply({ kind: 'release', touch: touch.identifier })
  })
  listener.passive(scroller, 'touchcancel', () => apply({ kind: 'cancel' }))

  // A layer that opens, a collection another client starts, the sources that go: a pull under
  // way is dropped.
  effect(() => {
    if (state().kind !== 'idle' && !options.enabled()) untracked(() => apply({ kind: 'cancel' }))
  })

  return state.asReadonly()
}

/** The touch `identifier` among those that changed. */
function touchOf(event: TouchEvent, identifier: number): Touch | null {
  return [...event.changedTouches].find(touch => touch.identifier === identifier) ?? null
}

/** An Apple Pencil's touch (Safari's `touchType`, which other browsers do not set). */
function isStylus(touch: Touch): boolean {
  return 'touchType' in touch && touch.touchType === 'stylus'
}
