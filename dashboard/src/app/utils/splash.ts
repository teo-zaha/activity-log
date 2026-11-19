/** On `<html>` while the splash is up; `_splash.scss` hides the header's brand under it. */
const SPLASHING = 'is-splashing'

/** Longest exit transition in `_splash.scss`, plus slack for a frame that comes late. */
const EXIT_MS = 400

/** When the app has not rendered by now, its start failed: the splash must not hide that. */
export const SPLASH_WATCHDOG_MS = 10_000

// Plain timers, media queries and listeners here, not Signality: the splash lives outside Angular
// (inlined in index.html, gone after the first render) and `watchSplash` runs before bootstrap,
// so there is no injection context for them.

/**
 * Dismisses the splash {@link SPLASH_WATCHDOG_MS} from now if the app has not done so: a start
 * that fails (bootstrap rejects, the first render throws) would otherwise loop it forever over a
 * blank or broken page. After a normal start this finds no splash and changes nothing.
 */
export function watchSplash(doc: Document = document): void {
  setTimeout(() => dismissSplash(doc), SPLASH_WATCHDOG_MS)
}

/**
 * Hands the startup splash inlined in `index.html` off to the app, at the end of its first cycle
 * or of the cycle that is running when the app is ready, so a cycle is never cut short (with
 * reduced motion there is no cycle and it leaves at once). The backdrop fades while each
 * `[data-splash-to]` part flies into the element its selector names, or fades when that is
 * missing or hidden; the header's own brand, hidden meanwhile, replaces them in the frame the
 * splash goes. Without a splash this only shows the header's brand; a second call does nothing.
 */
export function dismissSplash(doc: Document = document): void {
  const splash = doc.getElementById('splash')
  if (!splash) {
    doc.documentElement.classList.remove(SPLASHING)
    return
  }
  if (splash.hasAttribute('data-dismissing')) {
    return
  }
  splash.setAttribute('data-dismissing', '')

  const now = performance.now()
  // A timer, not Signality: see the note at the top of this file.
  setTimeout(() => leave(doc, splash), Math.max(0, cycleEnd(splash, now) - now))
}

/**
 * The first cycle end at or after `now`, on the `performance.now()` clock. The splash's looping
 * animation starts one cycle in and repeats every cycle, so its iterations are the cycle ends.
 * Without one (reduced motion, no animation support) this is `now`.
 */
function cycleEnd(splash: HTMLElement, now: number): number {
  const animations =
    typeof splash.getAnimations === 'function' ? splash.getAnimations({ subtree: true }) : []
  for (const animation of animations) {
    const timing = animation.effect?.getComputedTiming()
    const period = timing?.duration
    if (timing?.iterations !== Infinity || typeof period !== 'number' || period <= 0) {
      continue
    }
    const start = typeof animation.startTime === 'number' ? animation.startTime : now
    const first = start + (timing.delay ?? 0)
    return first + Math.max(0, Math.ceil((now - first) / period)) * period
  }
  return now
}

function leave(doc: Document, splash: HTMLElement): void {
  // Read once at the moment of leaving; no injection context needed (see the top of this file).
  const reduced = doc.defaultView?.matchMedia('(prefers-reduced-motion: reduce)').matches ?? false
  splash.classList.add('is-leaving')

  let flying = false
  for (const part of splash.querySelectorAll<HTMLElement>('[data-splash-to]')) {
    const selector = part.dataset['splashTo']
    const target = reduced || !selector ? null : doc.querySelector(selector)
    const to = target?.getBoundingClientRect()
    // A visually hidden target (the title in a narrow header) is a 1px box.
    if (!to || to.width < 4) {
      part.classList.add('splash-fade')
      continue
    }
    const from = part.getBoundingClientRect()
    const dx = to.left + to.width / 2 - (from.left + from.width / 2)
    const dy = to.top + to.height / 2 - (from.top + from.height / 2)
    part.style.transform = `translate(${dx}px, ${dy}px) scale(${to.width / from.width})`
    flying = true
  }

  // Same task, so same frame: the header's brand appears exactly where the flight landed.
  const remove = (): void => {
    doc.documentElement.classList.remove(SPLASHING)
    splash.remove()
  }
  // A flight ends with its transform; otherwise the backdrop (the splash's ::before) ends it. Other
  // properties of the splash can end transitions too (reduced motion transitions them all). A plain
  // listener: it goes with the node (see the top of this file).
  splash.addEventListener('transitionend', event => {
    if (flying ? event.propertyName === 'transform' : event.pseudoElement === '::before') {
      remove()
    }
  })
  // A background tab or a missed transitionend must not leave the layer in place (plain timer: see
  // the top of this file).
  setTimeout(remove, EXIT_MS)
}
