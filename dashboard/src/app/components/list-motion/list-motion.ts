import { LIST_MOTION } from './list-motion-options'

/** Limit animated changes in a replacement to the scale of one virtual window. */
const ANIMATED_CHANGES_MAX = 32

// FormKit AutoAnimate's timings with the settings Logs pane's duration and easing.
const MOVE = { ...LIST_MOTION, fill: 'both' } satisfies KeyframeAnimationOptions
const ADD = {
  duration: LIST_MOTION.duration * 1.5,
  easing: 'ease-in',
  fill: 'both',
} satisfies KeyframeAnimationOptions
const REMOVE = {
  duration: LIST_MOTION.duration,
  easing: 'ease-out',
  fill: 'both',
} satisfies KeyframeAnimationOptions

export interface PositionedItem {
  readonly element: HTMLElement
  readonly size: number
  /** Screen coordinates at a host-supplied scroll offset, including any sticky positioning. */
  readonly topAt: (scrollTop: number) => number
}

export interface ListMotionFrame {
  /** Changes only when the data changes, including for hosts that virtualize their items. */
  readonly revision: unknown
  readonly keys: Pick<ReadonlySet<string>, 'has'>
  readonly items: ReadonlyMap<string, PositionedItem>
  readonly viewport: { readonly top: number; readonly bottom: number }
  /** The host reports both offsets after it finishes anchoring the new layout. */
  readonly beforeScroll: number
  readonly scrollTop: number
  readonly anchor?: string
  readonly pauseOn: unknown
  readonly paused: boolean
  /** The host can supply a presentational copy for a removed item. */
  readonly ghost: (item: PositionedItem, top: number) => HTMLElement | null
}

interface Moving {
  readonly element: HTMLElement
  readonly animation: Animation
  readonly offset: number
}

interface Fading {
  readonly element: HTMLElement
  readonly animation: Animation
  readonly opacity: number
}

interface Ghost {
  readonly element: HTMLElement
  readonly animation: Animation
  readonly top: number
  readonly opacity: number
}

function onScreen(top: number, size: number, frame: ListMotionFrame): boolean {
  return top < frame.viewport.bottom && top + size > frame.viewport.top
}

function progress(animation: Animation): number {
  return animation.effect?.getComputedTiming().progress ?? 0
}

function onEnd(animation: Animation, clean: () => void): void {
  void animation.finished.then(clean, clean)
}

/**
 * Animates data changes between rendered frames. Transforms affect presentation only.
 * The next update samples unfinished transforms before cancelling them, so it starts where the
 * reader sees the items now.
 */
export class ListMotion {
  private previous?: ListMotionFrame
  private readonly moves = new Map<string, Moving>()
  private readonly fades = new Map<string, Fading>()
  private readonly ghosts = new Map<string, Ghost>()

  reset(): void {
    this.previous = undefined
    this.cancel()
  }

  update(next: ListMotionFrame): void {
    const before = this.previous
    this.previous = next
    if (!before || next.paused || before.pauseOn !== next.pauseOn) {
      this.cancel()
      return
    }
    if (before.paused) return
    if (next.anchor !== undefined) this.moves.get(next.anchor)?.animation.cancel()
    if (Object.is(before.revision, next.revision)) return

    let changed = 0
    const actions: (() => void)[] = []
    for (const [key, item] of before.items) {
      if (next.keys.has(key)) continue
      changed++
      const top = item.topAt(next.beforeScroll) + this.translation(key, item.element)
      if (onScreen(top, item.size, before)) {
        actions.push(() => this.remove(key, item, top, next.ghost))
      }
    }
    for (const [key, item] of next.items) {
      const old = before.items.get(key)
      const ghost = this.ghosts.get(key)
      const top = item.topAt(next.scrollTop)
      if (!before.keys.has(key)) {
        changed++
        if (!onScreen(top, item.size, next)) continue
        actions.push(() => {
          const opacity = ghost ? ghost.opacity * (1 - progress(ghost.animation)) : 0
          if (ghost) {
            this.dropGhost(key, ghost)
            if (key !== next.anchor) this.move(key, item.element, ghost.top - top)
          }
          this.add(key, item.element, opacity)
        })
      } else if (old && key !== next.anchor) {
        const oldTop = old.topAt(next.beforeScroll)
        if (Math.abs(oldTop - top) < 0.5) continue
        const from = oldTop + this.translation(key, old.element)
        if (!onScreen(from, old.size, before) && !onScreen(top, item.size, next)) continue
        changed++
        actions.push(() => this.move(key, item.element, from - top))
      }
    }
    if (changed + this.ghosts.size > ANIMATED_CHANGES_MAX) {
      this.cancel()
      return
    }
    for (const action of actions) action()
  }

  private cancel(): void {
    for (const { animation } of this.moves.values()) animation.cancel()
    for (const { animation } of this.fades.values()) animation.cancel()
    for (const [key, ghost] of this.ghosts) this.dropGhost(key, ghost)
    this.moves.clear()
    this.fades.clear()
  }

  private translation(key: string, element: HTMLElement): number {
    const move = this.moves.get(key)
    if (move?.element !== element || move.animation.playState === 'idle') return 0
    return move.offset * (1 - progress(move.animation))
  }

  private opacity(key: string, element: HTMLElement): number {
    const fade = this.fades.get(key)
    if (fade?.element !== element || fade.animation.playState === 'idle') return 1
    return fade.opacity + (1 - fade.opacity) * Math.max(0, 2 * progress(fade.animation) - 1)
  }

  private move(key: string, element: HTMLElement, offset: number): void {
    this.moves.get(key)?.animation.cancel()
    if (Math.abs(offset) < 0.5) return
    const animation = element.animate(
      [{ transform: `translateY(${offset}px)` }, { transform: 'none' }],
      MOVE
    )
    const move = { element, animation, offset }
    this.moves.set(key, move)
    onEnd(animation, () => {
      if (this.moves.get(key) !== move) return
      this.moves.delete(key)
      animation.cancel()
    })
  }

  private add(key: string, element: HTMLElement, opacity: number): void {
    this.fades.get(key)?.animation.cancel()
    const animation = element.animate([{ opacity }, { opacity, offset: 0.5 }, { opacity: 1 }], ADD)
    const fade = { element, animation, opacity }
    this.fades.set(key, fade)
    onEnd(animation, () => {
      if (this.fades.get(key) !== fade) return
      this.fades.delete(key)
      animation.cancel()
    })
  }

  private remove(
    key: string,
    item: PositionedItem,
    top: number,
    copy: ListMotionFrame['ghost']
  ): void {
    const opacity = this.opacity(key, item.element)
    this.moves.get(key)?.animation.cancel()
    this.fades.get(key)?.animation.cancel()
    const old = this.ghosts.get(key)
    if (old) this.dropGhost(key, old)
    const element = copy(item, top)
    if (!element) return
    const animation = element.animate([{ opacity }, { opacity: 0 }], REMOVE)
    const ghost = { element, animation, top, opacity }
    this.ghosts.set(key, ghost)
    onEnd(animation, () => {
      if (this.ghosts.get(key) === ghost) this.dropGhost(key, ghost)
    })
  }

  private dropGhost(key: string, ghost: Ghost): void {
    this.ghosts.delete(key)
    ghost.animation.cancel()
    ghost.element.remove()
  }
}
