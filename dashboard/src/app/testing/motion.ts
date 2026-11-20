/** The classes `NavigationMotionService` gives what a navigation moves. */
const MOVING = '.motion-switch, .motion-page-in, .motion-page-out'

/**
 * jsdom runs no animations, so what a navigation moves would come and go at once. This holds
 * each moving element's own animation, as a slide would, until `finish`; `restore` puts jsdom's
 * back (`test-polyfills.ts`).
 */
export function holdMotion(): { finish(): void; restore(): void } {
  const original = Element.prototype.getAnimations
  let finish: () => void = () => undefined
  const finished = new Promise<Animation>(resolve => (finish = (): void => resolve(slide)))
  // Only `finished` is read; jsdom has no `Animation` to make one from.
  const slide = { finished } as Animation
  Element.prototype.getAnimations = function (this: Element): Animation[] {
    return this.matches(MOVING) ? [slide] : original.call(this)
  }
  return {
    finish: (): void => finish(),
    restore: (): void => {
      Element.prototype.getAnimations = original
    },
  }
}
