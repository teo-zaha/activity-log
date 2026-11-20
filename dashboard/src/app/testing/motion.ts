/** The classes `NavigationMotionService` gives what a navigation moves. */
const MOVING = ['motion-switch', 'motion-page-in', 'motion-page-out']
const TRANSITION = /^(?:page|view)-(?:enter|leave)$|^motion-/

/**
 * A slide, as a browser runs one for a moving element: held (`pause`) and resumed (`play`) by the
 * motion service, ended by `finish`, or cancelled when the element's motion changes before it
 * ended. It counts what was asked of it, so a spec sees a slide played after it was cancelled.
 */
export class HeldSlide {
  playState: AnimationPlayState = 'running'
  /** Each `play()`, including one that would restart it after it was cancelled. */
  plays = 0
  readonly finished: Promise<Animation>
  private settle: { resolve(): void; reject(): void } = { resolve: () => {}, reject: () => {} }

  constructor(readonly transition: string) {
    this.finished = new Promise<Animation>((resolve, reject) => {
      this.settle = {
        resolve: (): void => resolve(this as unknown as Animation),
        reject: (): void => reject(new DOMException('The slide was cancelled', 'AbortError')),
      }
    })
    // A cancelled slide's rejection is the motion service's to read.
    this.finished.catch(() => undefined)
  }

  pause(): void {
    if (this.playState === 'running') this.playState = 'paused'
  }

  play(): void {
    this.plays++
    this.playState = 'running'
  }

  finish(): void {
    if (this.playState === 'idle' || this.playState === 'finished') return
    this.playState = 'finished'
    this.settle.resolve()
  }

  cancel(): void {
    if (this.playState === 'idle' || this.playState === 'finished') return
    this.playState = 'idle'
    this.settle.reject()
  }
}

/** The motion classes an element holds now, as one key: what its slide is for. */
function transitionOf(element: Element): string {
  return [...element.classList].filter(name => TRANSITION.test(name)).join(' ')
}

/**
 * jsdom runs no animations, so what a navigation moves would come and go at once. This gives each
 * moving element a slide of its own (`HeldSlide`), held until `finish`. Like a browser, a slide
 * whose element changes its motion before it ended (an entrance that leaves) is cancelled and no
 * longer the element's; the new motion gets a new one. `restore` puts jsdom's back
 * (`test-polyfills.ts`).
 */
export function holdMotion(): {
  finish(): void
  restore(): void
  slidesOf(element: Element): readonly HeldSlide[]
} {
  const original = Element.prototype.getAnimations
  const slides = new Map<Element, HeldSlide[]>()
  Element.prototype.getAnimations = function (this: Element): Animation[] {
    if (!MOVING.some(name => this.classList.contains(name))) return original.call(this)
    const own = slides.get(this) ?? []
    const transition = transitionOf(this)
    const current = own.at(-1)
    if (current?.transition === transition) {
      return current.playState === 'idle' ? [] : [current as unknown as Animation]
    }
    current?.cancel()
    const slide = new HeldSlide(transition)
    slides.set(this, [...own, slide])
    return [slide as unknown as Animation]
  }
  return {
    finish: (): void => slides.forEach(own => own.forEach(slide => slide.finish())),
    restore: (): void => {
      Element.prototype.getAnimations = original
    },
    slidesOf: element => slides.get(element) ?? [],
  }
}
