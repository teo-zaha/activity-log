import { inputBinding, signal } from '@angular/core'
import { type DirectiveFixture, TestBed } from '@angular/core/testing'
import type * as AutoAnimate from '@formkit/auto-animate'
import {
  type MockInstance,
  afterAll,
  afterEach,
  beforeAll,
  beforeEach,
  describe,
  expect,
  it,
  vi,
} from 'vitest'
import { AUTO_ANIMATE, AutoAnimateDirective } from './auto-animate.directive'
import { liveIntersectionObservers, stubCountingObservers } from './layout-observers.testing'

/**
 * A copy of AutoAnimate of its own, evaluated with the counting observers in place: the app's copy
 * is shared by the spec files of a worker and does nothing in jsdom, which has no `ResizeObserver`.
 */
async function loadAutoAnimate(): Promise<typeof AutoAnimate.autoAnimate> {
  // Not `new URL(…, import.meta.url)`, which the build would take for an asset.
  const dashboard = import.meta.url.slice(0, import.meta.url.indexOf('/src/app/'))
  const url = `${dashboard}/node_modules/@formkit/auto-animate/index.mjs?instance=spec`
  const module: typeof AutoAnimate = await import(/* @vite-ignore */ url)
  return module.autoAnimate
}

describe('AutoAnimateDirective', () => {
  const enabled = signal(true)
  const filter = signal('all')
  let started: MockInstance<typeof setInterval>
  let cleared: MockInstance<typeof clearInterval>

  let autoAnimate: typeof AutoAnimate.autoAnimate

  beforeAll(async () => {
    stubCountingObservers()
    autoAnimate = await loadAutoAnimate()
  })

  afterAll(() => vi.unstubAllGlobals())

  /** Intervals started and not cleared: AutoAnimate's position polls. */
  function liveIntervals(): number {
    const clearedIds = new Set(cleared.mock.calls.map(([id]) => id))
    return started.mock.results.filter(
      result => result.type === 'return' && !clearedIds.has(result.value)
    ).length
  }

  /** The host is empty when the directive is created, as in a template; its rows come after. */
  function create(rows = 3): DirectiveFixture<unknown> {
    const fixture = TestBed.createDirective(AutoAnimateDirective, {
      tagName: 'div',
      bindings: [
        inputBinding('appAutoAnimate', enabled),
        inputBinding('appAutoAnimatePauseOn', filter),
      ],
    })
    fixture.nativeElement.append(...Array.from({ length: rows }, () => document.createElement('p')))
    return fixture
  }

  /** Renders, then lets the mutation observer, the deferred resume and the timers run. */
  async function settle(fixture: DirectiveFixture<unknown>, ms = 0): Promise<void> {
    fixture.detectChanges()
    await vi.advanceTimersByTimeAsync(ms)
  }

  beforeEach(() => {
    TestBed.configureTestingModule({
      providers: [{ provide: AUTO_ANIMATE, useValue: autoAnimate }],
    })
    enabled.set(true)
    filter.set('all')
    vi.useFakeTimers({
      toFake: [
        'setTimeout',
        'clearTimeout',
        'setInterval',
        'clearInterval',
        'requestAnimationFrame',
        'cancelAnimationFrame',
        'Date',
      ],
    })
    started = vi.spyOn(globalThis, 'setInterval')
    cleared = vi.spyOn(globalThis, 'clearInterval')
  })

  afterEach(() => {
    vi.useRealTimers()
    vi.restoreAllMocks()
  })

  it('polls only the host, never the rows', async () => {
    const fixture = create()
    await settle(fixture, 2_500)

    expect(started).toHaveBeenCalledTimes(1)
    expect(liveIntersectionObservers()).toBeGreaterThan(1)
    fixture.destroy()
  })

  it('leaves no interval running after pauses, and nothing after destroy', async () => {
    const fixture = create()
    await settle(fixture)

    for (const value of ['warn', 'error', 'all', 'warn', 'error']) {
      filter.set(value)
      fixture.nativeElement.append(document.createElement('p'))
      fixture.nativeElement.firstElementChild?.remove()
      await settle(fixture, 600)
    }
    await settle(fixture, 2_500)
    expect(liveIntervals()).toBe(0)

    fixture.destroy()
    await vi.advanceTimersByTimeAsync(10_000)
    expect(liveIntervals()).toBe(0)
    expect(liveIntersectionObservers()).toBe(0)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('clears the host poll that starts after an early destroy', async () => {
    const fixture = create()
    await settle(fixture)
    fixture.destroy()

    await vi.advanceTimersByTimeAsync(10_000)
    expect(started).toHaveBeenCalledTimes(1)
    expect(liveIntervals()).toBe(0)
    expect(liveIntersectionObservers()).toBe(0)
    expect(vi.getTimerCount()).toBe(0)
  })

  it('stops polling while off, and does not set up again when turned back on', async () => {
    const fixture = create()
    await settle(fixture)
    enabled.set(false)
    await settle(fixture, 2_500)
    expect(liveIntervals()).toBe(0)

    enabled.set(true)
    await settle(fixture, 2_500)
    expect(started).toHaveBeenCalledTimes(1)
    expect(liveIntervals()).toBe(0)

    fixture.destroy()
    await vi.advanceTimersByTimeAsync(10_000)
    expect(vi.getTimerCount()).toBe(0)
  })
})
