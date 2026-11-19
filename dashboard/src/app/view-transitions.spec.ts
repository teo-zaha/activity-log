import { provideLocationMocks } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { DefaultUrlSerializer, Router } from '@angular/router'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { provideAppRouter } from './app.routes'
import { viewTransitionKind } from './view-transitions'

const serializer = new DefaultUrlSerializer()
const kind = (from: string, to: string, phone = false): string =>
  viewTransitionKind(serializer.parse(from), serializer.parse(to), phone)

describe('viewTransitionKind', () => {
  it('cross-fades between calendar views only', () => {
    expect(kind('/week/2026-11-17', '/day/2026-11-19')).toBe('switch')
    expect(kind('/timeline', '/day/2026-11-17')).toBe('none')
    expect(kind('/week/2026-11-17', '/timeline')).toBe('none')
    expect(kind('/day/2026-11-17', '/day/2026-11-18')).toBe('none')
  })

  it('keeps still for query changes on a larger screen', () => {
    expect(kind('/day/2026-11-17', '/day/2026-11-17?event=git%2Fa%2F1')).toBe('none')
    expect(kind('/timeline', '/timeline?settings=home')).toBe('none')
    expect(kind('/timeline', '/timeline?q=x', true)).toBe('none')
  })

  describe('pages on a phone', () => {
    const day = '/day/2026-11-17'
    const group = 'events=git%2Fa%2F1&events=git%2Fa%2F2'

    it('slides event details in and out, but steps through a group in place', () => {
      expect(kind(day, `${day}?event=git%2Fa%2F1`, true)).toBe('page-in')
      expect(kind(`/timeline?event=git%2Fa%2F1`, '/timeline', true)).toBe('page-out')
      expect(kind(`${day}?${group}`, `${day}?${group}&event=git%2Fa%2F2`, true)).toBe('page-in')
      expect(kind(`${day}?${group}&event=git%2Fa%2F2`, `${day}?${group}`, true)).toBe('page-out')
      expect(
        kind(`${day}?${group}&event=git%2Fa%2F1`, `${day}?${group}&event=git%2Fa%2F2`, true)
      ).toBe('none')
    })

    it('slides between the settings home and its sections', () => {
      expect(kind(`${day}?settings=home`, `${day}?settings=logs`, true)).toBe('page-in')
      expect(kind(`${day}?settings=gitlab%2Foss`, `${day}?settings=home`, true)).toBe('page-out')
      expect(kind(`${day}?settings=logs`, `${day}?settings=general`, true)).toBe('none')
    })

    it('opens and leaves the settings in place, as a tab of the bottom bar', () => {
      expect(kind(day, `${day}?settings=home`, true)).toBe('none')
      expect(kind(day, `${day}?settings=logs`, true)).toBe('none')
      expect(kind(`${day}?settings=home`, day, true)).toBe('none')
      expect(kind(`${day}?settings=gitlab%2Foss`, day, true)).toBe('none')
    })
  })
})

describe('the view transition gate', () => {
  const start = vi.fn((update?: ViewTransitionUpdateCallback | StartViewTransitionOptions) => {
    const callback = typeof update === 'function' ? update : update?.update
    const done = Promise.resolve().then(async () => {
      await callback?.()
    })
    return {
      updateCallbackDone: done,
      ready: done,
      finished: done,
      types: new Set<string>(),
      skipTransition: (): void => undefined,
    }
  })

  beforeEach(() => {
    start.mockReset()
    document.startViewTransition = start
  })
  afterEach(() => {
    Reflect.deleteProperty(document, 'startViewTransition')
  })

  it('starts a transition only for navigations that animate', async () => {
    TestBed.configureTestingModule({ providers: [provideAppRouter(), provideLocationMocks()] })
    const router = TestBed.inject(Router)
    router.initialNavigation()
    await router.navigateByUrl('/week/2026-11-17')
    start.mockClear()

    await router.navigateByUrl('/week/2026-11-17?q=x')
    await router.navigateByUrl('/week/2026-11-24')
    await router.navigateByUrl('/timeline')
    expect(start).not.toHaveBeenCalled()

    await router.navigateByUrl('/week/2026-11-24')
    await router.navigateByUrl('/day/2026-11-24')
    expect(start).toHaveBeenCalledOnce()
    expect(router.url).toBe('/day/2026-11-24')
  })

  it('answers navigations without a transition with a resolved one, so nothing is logged', async () => {
    const logged = vi.spyOn(console, 'error').mockImplementation(() => undefined)
    TestBed.configureTestingModule({ providers: [provideAppRouter(), provideLocationMocks()] })
    const router = TestBed.inject(Router)
    router.initialNavigation()
    await router.navigateByUrl('/week/2026-11-17')
    await router.navigateByUrl('/week/2026-11-17?q=x')
    await new Promise(resolve => setTimeout(resolve))

    expect(logged).not.toHaveBeenCalled()
    await expect(document.startViewTransition(() => undefined).ready).resolves.toBeUndefined()
    logged.mockRestore()
  })

  it('keeps the kind of a transition that started while another one was finishing', async () => {
    const finishes: (() => void)[] = []
    start.mockImplementation(update => {
      const callback = typeof update === 'function' ? update : update?.update
      const done = Promise.resolve().then(async () => {
        await callback?.()
      })
      const finished = new Promise<void>(resolve => finishes.push(resolve))
      return {
        updateCallbackDone: done,
        ready: done,
        finished,
        types: new Set<string>(),
        skipTransition: (): void => undefined,
      }
    })
    TestBed.configureTestingModule({ providers: [provideAppRouter(), provideLocationMocks()] })
    const router = TestBed.inject(Router)
    const root = document.documentElement
    router.initialNavigation()
    await router.navigateByUrl('/week/2026-11-17')

    await router.navigateByUrl('/day/2026-11-17')
    await router.navigateByUrl('/week/2026-11-17')
    expect(finishes).toHaveLength(2)

    finishes[0]?.()
    await new Promise(resolve => setTimeout(resolve))
    expect(root.dataset['viewTransition']).toBe('switch')
    finishes[1]?.()
    await new Promise(resolve => setTimeout(resolve))
    expect(root.dataset['viewTransition']).toBeUndefined()
  })
})
