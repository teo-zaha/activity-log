import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { SPLASH_WATCHDOG_MS, dismissSplash, watchSplash } from './splash'

const SPLASH = `
  <div id="splash">
    <div class="splash-fly" data-splash-to="app-header .brand-mark"></div>
    <div class="splash-fly" data-splash-to="app-header .brand-title"></div>
  </div>`

const rect = (left: number, top: number, width: number, height: number): DOMRect =>
  DOMRect.fromRect({ x: left, y: top, width, height })

const transitionEnd = (target: Element, propertyName: string, pseudoElement = ''): void => {
  target.dispatchEvent(
    Object.assign(new Event('transitionend', { bubbles: true }), { propertyName, pseudoElement })
  )
}

/** Gives the splash a loop that starts at `startTime` and repeats every `cycle` ms after one cycle. */
const withLoop = (startTime: number, cycle: number): void => {
  const splash = document.getElementById('splash')
  if (!splash) {
    throw new Error('no splash')
  }
  const timing = { iterations: Infinity, delay: cycle, duration: cycle }
  const animation = { startTime, effect: { getComputedTiming: (): typeof timing => timing } }
  splash.getAnimations = (): Animation[] => [animation as unknown as Animation]
}

describe('dismissSplash', () => {
  const splash = (): HTMLElement | null => document.getElementById('splash')
  const parts = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('.splash-fly')]

  beforeEach(() => {
    vi.useFakeTimers()
    vi.spyOn(performance, 'now').mockReturnValue(100)
    document.body.innerHTML = SPLASH
    document.documentElement.classList.add('is-splashing')
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
    document.body.innerHTML = ''
    document.documentElement.classList.remove('is-splashing')
  })

  const splashing = (): boolean => document.documentElement.classList.contains('is-splashing')
  const leaving = (): boolean => splash()?.classList.contains('is-leaving') ?? false

  it('plays the whole first cycle when the app is ready early', () => {
    withLoop(20, 1100)
    dismissSplash()

    vi.advanceTimersByTime(1019)
    expect(leaving()).toBe(false)
    vi.advanceTimersByTime(1)
    expect(leaving()).toBe(true)
  })

  it('finishes the cycle that is running when the app is ready late', () => {
    vi.mocked(performance.now).mockReturnValue(2500)
    withLoop(20, 1100)
    dismissSplash()

    vi.advanceTimersByTime(819)
    expect(leaving()).toBe(false)
    vi.advanceTimersByTime(1)
    expect(leaving()).toBe(true)
  })

  it('leaves at once on a cycle end', () => {
    vi.mocked(performance.now).mockReturnValue(2220)
    withLoop(20, 1100)
    dismissSplash()

    vi.advanceTimersByTime(0)
    expect(leaving()).toBe(true)
  })

  it('leaves at once without a cycle (reduced motion)', () => {
    dismissSplash()

    vi.advanceTimersByTime(0)
    expect(leaving()).toBe(true)
  })

  it('flies each part into the header brand and removes the splash when it lands', () => {
    document.body.insertAdjacentHTML(
      'beforeend',
      '<app-header><span class="brand-mark"></span><span class="brand-title"></span></app-header>'
    )
    const [mark, title] = [...document.querySelectorAll('app-header span')]
    mark.getBoundingClientRect = (): DOMRect => rect(24, 12, 34, 34)
    title.getBoundingClientRect = (): DOMRect => rect(68, 18, 100, 20)
    const [markPart, titlePart] = parts()
    markPart.getBoundingClientRect = (): DOMRect => rect(604, 300, 72, 72)
    titlePart.getBoundingClientRect = (): DOMRect => rect(574, 390, 132, 26)

    dismissSplash()
    vi.advanceTimersByTime(0)

    expect(markPart.style.transform).toBe(`translate(-599px, -307px) scale(${34 / 72})`)
    expect(titlePart.style.transform).toContain('translate(')
    transitionEnd(splash() as HTMLElement, 'opacity', '::before')
    expect(splash()).not.toBeNull()
    expect(splashing()).toBe(true)
    transitionEnd(markPart, 'transform')
    expect(splash()).toBeNull()
    expect(splashing()).toBe(false)
  })

  it('fades parts whose header target is missing and leaves when the backdrop has faded', () => {
    dismissSplash()
    vi.advanceTimersByTime(0)

    expect(parts().every(part => part.classList.contains('splash-fade'))).toBe(true)
    expect(parts().every(part => part.style.transform === '')).toBe(true)
    transitionEnd(splash() as HTMLElement, 'color')
    expect(splash()).not.toBeNull()
    transitionEnd(splash() as HTMLElement, 'opacity', '::before')
    expect(splash()).toBeNull()
  })

  it('fades instead of flying under reduced motion', () => {
    vi.spyOn(window, 'matchMedia').mockReturnValue({ matches: true } as MediaQueryList)
    document.body.insertAdjacentHTML(
      'beforeend',
      '<app-header><span class="brand-mark"></span></app-header>'
    )
    const mark = document.querySelector('app-header span')
    if (mark) {
      mark.getBoundingClientRect = (): DOMRect => rect(24, 12, 34, 34)
    }

    dismissSplash()
    vi.advanceTimersByTime(0)

    expect(parts().every(part => part.classList.contains('splash-fade'))).toBe(true)
  })

  it('removes the splash even when no transition ends', () => {
    dismissSplash()
    vi.advanceTimersByTime(400)
    expect(splash()).toBeNull()
  })

  it('dismisses the splash when the app never renders', () => {
    watchSplash()

    vi.advanceTimersByTime(SPLASH_WATCHDOG_MS - 1)
    expect(leaving()).toBe(false)
    // The watchdog fires; the leave it schedules from inside its callback is a nested timer.
    vi.advanceTimersByTime(2)
    expect(leaving()).toBe(true)
    vi.advanceTimersByTime(400)
    expect(splash()).toBeNull()
    expect(splashing()).toBe(false)
  })

  it('leaves a normal start alone when the watchdog fires', () => {
    watchSplash()
    dismissSplash()
    vi.advanceTimersByTime(400)
    expect(splash()).toBeNull()

    document.body.innerHTML = '<p id="later"></p>'
    expect(() => vi.advanceTimersByTime(SPLASH_WATCHDOG_MS)).not.toThrow()
    expect(document.getElementById('later')).not.toBeNull()
    expect(splashing()).toBe(false)
  })

  it('shows the header brand when there is no splash', () => {
    document.body.innerHTML = ''
    dismissSplash()
    expect(splashing()).toBe(false)
  })
})
