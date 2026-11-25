/**
 * Waits for fonts and finite animations on the document timeline. Scroll-driven animations and
 * repeating spinners can stay active while the layout is ready to measure.
 */
export async function waitForLayout(): Promise<void> {
  for (;;) {
    await new Promise(resolve => requestAnimationFrame(resolve))
    // Flush layout so the browser knows which fonts the newly rendered content uses.
    document.documentElement.getBoundingClientRect()
    await document.fonts.ready
    const animations = document
      .getAnimations()
      .filter(
        animation =>
          animation.timeline instanceof DocumentTimeline &&
          (animation.pending ||
            animation.playState === 'running' ||
            animation.playState === 'paused') &&
          animation.effect?.getComputedTiming().endTime !== Infinity
      )
    if (animations.length === 0) return
    // A navigation can cancel an animation as it replaces the element that was moving.
    await Promise.allSettled(animations.map(animation => animation.finished))
  }
}
