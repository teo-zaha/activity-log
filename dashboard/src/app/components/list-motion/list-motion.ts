import { DestroyRef, EnvironmentInjector, type Signal, computed, inject } from '@angular/core'
import { mediaQuery } from '@signality/core/browser/media-query'
import { watcher } from '@signality/core/reactivity/watcher'
import { LIST_MOTION } from './list-motion-options'

/** Avoid a screenful of simultaneous animations when a collection changes many items. */
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
  /** The full layout box is available for DOM-positioned items that can also slide sideways or resize. */
  readonly across?: { readonly left: number; readonly width: number }
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
  /** The host can supply a presentational copy for a removed item. */
  readonly ghost: (item: PositionedItem, top: number) => HTMLElement | null
}

interface Moving {
  readonly element: HTMLElement
  readonly animation: Animation
  readonly x: number
  readonly y: number
  readonly scaleX: number
  readonly scaleY: number
}

interface ItemBox {
  readonly left: number
  readonly top: number
  readonly width: number
  readonly height: number
}

interface Fading {
  readonly element: HTMLElement
  readonly animation: Animation
  readonly opacity: number
}

interface Ghost {
  readonly element: HTMLElement
  readonly animation: Animation
  readonly box: ItemBox
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

function boxAt(item: PositionedItem, scrollTop: number): ItemBox {
  return {
    top: item.topAt(scrollTop),
    left: item.across?.left ?? 0,
    width: item.across?.width ?? 0,
    height: item.size,
  }
}

function moved(from: ItemBox, to: ItemBox): boolean {
  return (
    Math.abs(from.top - to.top) >= 0.5 ||
    (to.width > 0 &&
      (Math.abs(from.left - to.left) >= 0.5 ||
        Math.abs(from.width - to.width) >= 0.5 ||
        Math.abs(from.height - to.height) >= 0.5))
  )
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
  private readonly reducedMotion = mediaQuery('(prefers-reduced-motion: reduce)')
  private readonly disabled = computed(() => this.reducedMotion() || this.paused?.() === true)

  constructor(
    private readonly host: HTMLElement,
    private readonly paused?: Signal<boolean>
  ) {
    // Route reuse can remove a host from change detection while its animations still run.
    const pauseWatcher = watcher(
      this.disabled,
      paused => {
        if (paused) this.cancel()
      },
      { injector: inject(EnvironmentInjector) }
    )
    inject(DestroyRef).onDestroy(() => {
      pauseWatcher.destroy()
      this.reset()
    })
  }

  reset(): void {
    this.previous = undefined
    this.cancel()
  }

  /** Removes screen-positioned copies when the host scrolls independently of a data change. */
  dropGhosts(): void {
    for (const [key, ghost] of this.ghosts) this.dropGhost(key, ghost)
  }

  update(next: ListMotionFrame): void {
    const before = this.previous
    this.previous = next
    if (
      !before ||
      this.disabled() ||
      !this.host.isConnected ||
      typeof this.host.animate !== 'function'
    ) {
      this.cancel()
      return
    }
    if (next.anchor !== undefined) this.moves.get(next.anchor)?.animation.cancel()
    if (Object.is(before.revision, next.revision)) return

    const actions: (() => void)[] = []
    for (const [key, item] of before.items) {
      if (next.keys.has(key)) continue
      const box = this.visualBox(key, item, next.beforeScroll)
      if (onScreen(box.top, box.height, before)) {
        actions.push(() => this.remove(key, item, box, next.ghost))
      }
    }
    for (const [key, item] of next.items) {
      const old = before.items.get(key)
      const ghost = this.ghosts.get(key)
      const to = boxAt(item, next.scrollTop)
      if (!before.keys.has(key)) {
        if (!onScreen(to.top, to.height, next)) continue
        actions.push(() => {
          const opacity = ghost ? ghost.opacity * (1 - progress(ghost.animation)) : 0
          if (ghost) {
            this.dropGhost(key, ghost)
            if (key !== next.anchor) this.move(key, item.element, ghost.box, to)
          }
          this.add(key, item.element, opacity)
        })
      } else if (old?.element === item.element && key !== next.anchor) {
        if (!moved(boxAt(old, next.beforeScroll), to)) continue
        const from = this.visualBox(key, old, next.beforeScroll)
        if (!onScreen(from.top, from.height, before) && !onScreen(to.top, to.height, next)) continue
        actions.push(() => this.move(key, item.element, from, to))
      }
    }
    if (actions.length > ANIMATED_CHANGES_MAX) {
      this.cancel()
      return
    }
    for (const action of actions) action()
  }

  private cancel(): void {
    for (const { animation } of this.moves.values()) animation.cancel()
    for (const { animation } of this.fades.values()) animation.cancel()
    this.dropGhosts()
    this.moves.clear()
    this.fades.clear()
  }

  private visualBox(key: string, item: PositionedItem, scrollTop: number): ItemBox {
    const box = boxAt(item, scrollTop)
    const move = this.moves.get(key)
    if (move?.element !== item.element || move.animation.playState === 'idle') return box
    const remaining = 1 - progress(move.animation)
    return {
      top: box.top + move.y * remaining,
      left: box.left + move.x * remaining,
      width: box.width * (1 + (move.scaleX - 1) * remaining),
      height: box.height * (1 + (move.scaleY - 1) * remaining),
    }
  }

  private opacity(key: string, element: HTMLElement): number {
    const fade = this.fades.get(key)
    if (fade?.element !== element || fade.animation.playState === 'idle') return 1
    return fade.opacity + (1 - fade.opacity) * Math.max(0, 2 * progress(fade.animation) - 1)
  }

  private move(key: string, element: HTMLElement, from: ItemBox, to: ItemBox): void {
    this.moves.get(key)?.animation.cancel()
    if (!moved(from, to)) return
    const x = from.left - to.left
    const y = from.top - to.top
    const scaleX = to.width > 0 ? from.width / to.width : 1
    const scaleY = to.width > 0 && to.height > 0 ? from.height / to.height : 1
    const start =
      to.width > 0
        ? {
            transform: `translate(${x}px, ${y}px) scale(${scaleX}, ${scaleY})`,
            transformOrigin: 'top left',
          }
        : { transform: `translateY(${y}px)` }
    const animation = element.animate([start, { ...start, transform: 'none' }], MOVE)
    const move = { element, animation, x, y, scaleX, scaleY }
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
    box: ItemBox,
    copy: ListMotionFrame['ghost']
  ): void {
    const opacity = this.opacity(key, item.element)
    this.moves.get(key)?.animation.cancel()
    this.fades.get(key)?.animation.cancel()
    const old = this.ghosts.get(key)
    if (old) this.dropGhost(key, old)
    const visualItem = item.across
      ? { ...item, size: box.height, across: { left: box.left, width: box.width } }
      : item
    const element = copy(visualItem, box.top)
    if (!element) return
    const animation = element.animate([{ opacity }, { opacity: 0 }], REMOVE)
    const ghost = { element, animation, box, opacity }
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
