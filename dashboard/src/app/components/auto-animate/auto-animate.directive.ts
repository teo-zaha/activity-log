import {
  DestroyRef,
  Directive,
  ElementRef,
  InjectionToken,
  Injector,
  type OnChanges,
  type SimpleChanges,
  afterNextRender,
  afterRenderEffect,
  booleanAttribute,
  inject,
  input,
  untracked,
} from '@angular/core'
import { autoAnimate } from '@formkit/auto-animate'

import { LIST_MOTION } from '../list-motion/list-motion-options'

/**
 * AutoAnimate's setup. Its module checks for `ResizeObserver` once, when it is first evaluated, so
 * a spec gives a copy it loaded with layout observers in place.
 */
export const AUTO_ANIMATE = new InjectionToken<typeof autoAnimate>('AUTO_ANIMATE', {
  factory: (): typeof autoAnimate => autoAnimate,
})

/** AutoAnimate (0.10) starts its position poll of an element at a random time 0–2 s after setup. */
const POLL_START_MS = 2_100

/**
 * Animates the host's children as they are added, removed or moved (FormKit AutoAnimate). Off
 * under `prefers-reduced-motion`.
 *
 * Our own directive: the package's `@formkit/auto-animate/angular` is not AOT-compiled, injects
 * through an undecorated constructor parameter, and never disables its controller.
 *
 * AutoAnimate sets up when the host has no children yet, and its controller lives as long as the
 * directive; pauses and `[appAutoAnimate]="false"` disable it. That way only the host is ever
 * polled: AutoAnimate polls every child present at setup with an interval it clears only for
 * children still there, and starts each after a delay, so a disable or destroy can come first.
 * The host's own poll is cleared once it has started, and the controller destroyed after it.
 */
@Directive({ selector: '[appAutoAnimate]' })
export class AutoAnimateDirective implements OnChanges {
  /**
   * `[appAutoAnimate]="false"` turns it off: nothing animates and nothing polls. Long lists turn
   * it on only where they are seen.
   */
  readonly enabled = input(true, { alias: 'appAutoAnimate', transform: booleanAttribute })

  /**
   * A value that swaps the whole list when it changes, such as a filter: that render is not
   * animated, so only the changes in between (new rows, removed rows) are.
   */
  readonly pauseOn = input<unknown>(undefined, { alias: 'appAutoAnimatePauseOn' })

  private readonly injector = inject(Injector)
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
  private readonly controller = inject(AUTO_ANIMATE)(this.host, LIST_MOTION)
  /** The first render is one: its children are there from the start, not added. */
  private pauses = 1
  private tracking = false
  private settled = false
  private destroyed = false

  constructor() {
    this.resumeAfterRender()
    afterNextRender(() => {
      // As AutoAnimate does when it is set up on a rendered host, which this one may not have been.
      if (getComputedStyle(this.host).position === 'static') this.host.style.position = 'relative'
    })
    afterRenderEffect(() => {
      this.enabled()
      untracked(() => this.apply())
    })
    // A plain timer: it must outlive the directive to destroy the controller after the host's poll
    // has started, which Signality's timers do not.
    setTimeout(() => {
      this.settled = true
      if (this.destroyed) this.controller.destroy?.()
      else if (!this.wanted()) this.controller.disable()
    }, POLL_START_MS)
    inject(DestroyRef).onDestroy(() => {
      this.destroyed = true
      this.controller.disable()
      if (this.settled) this.controller.destroy?.()
    })
  }

  // Inputs are set before the host's children render, so disabling here comes before the swap.
  ngOnChanges(changes: SimpleChanges): void {
    const change = changes['pauseOn']
    if (!change || change.firstChange) return
    this.pauses++
    this.controller.disable()
    this.resumeAfterRender()
  }

  /**
   * AutoAnimate learns of a render from its mutation observer, whose callback runs as a microtask
   * after the render; resuming waits until after it.
   */
  private resumeAfterRender(): void {
    afterNextRender(
      () =>
        queueMicrotask(() => {
          this.pauses--
          if (this.pauses === 0 && !this.tracking) this.trackChildren()
          this.apply()
        }),
      { injector: this.injector }
    )
  }

  /**
   * Has AutoAnimate measure the first children and watch their position, as it does with the
   * children it finds at setup. It re-measures a parent's children when a CSS animation on it ends.
   */
  private trackChildren(): void {
    this.tracking = true
    this.host.dispatchEvent(new Event('animationend'))
  }

  private wanted(): boolean {
    return this.enabled() && this.pauses === 0 && !this.destroyed
  }

  private apply(): void {
    if (this.wanted()) this.controller.enable()
    else this.controller.disable()
  }
}
