import { provideLocationMocks } from '@angular/common/testing'
import { Location } from '@angular/common'
import { ApplicationRef } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { DefaultUrlSerializer, NavigationEnd, Router } from '@angular/router'
import { filter, firstValueFrom } from 'rxjs'
import { describe, expect, it, vi } from 'vitest'
import { provideAppRouter } from './app.routes'
import { type MotionTarget, NavigationMotionService, navigationMotion } from './navigation-motion'

const serializer = new DefaultUrlSerializer()
const motion = (from: string, to: string): string =>
  navigationMotion(serializer.parse(from), serializer.parse(to))

describe('navigationMotion', () => {
  it('cross-fades between calendar views only', () => {
    expect(motion('/week/2026-11-17', '/day/2026-11-19')).toBe('switch')
    expect(motion('/timeline', '/day/2026-11-17')).toBe('none')
    expect(motion('/week/2026-11-17', '/timeline')).toBe('none')
    expect(motion('/day/2026-11-17', '/day/2026-11-18')).toBe('none')
  })

  it('keeps still for other query changes', () => {
    expect(motion('/timeline', '/timeline?q=x')).toBe('none')
    expect(motion('/day/2026-11-17', '/day/2026-11-17?from=2026-11-01&to=2026-11-18')).toBe('none')
  })

  describe('pages', () => {
    const day = '/day/2026-11-17'
    const group = 'events=git%2Fa%2F1&events=git%2Fa%2F2'

    it('moves event details in and out, but steps through a group in place', () => {
      expect(motion(day, `${day}?event=git%2Fa%2F1`)).toBe('page-in')
      expect(motion(`/timeline?event=git%2Fa%2F1`, '/timeline')).toBe('page-out')
      expect(motion(`${day}?${group}`, `${day}?${group}&event=git%2Fa%2F2`)).toBe('page-in')
      expect(motion(`${day}?${group}&event=git%2Fa%2F2`, `${day}?${group}`)).toBe('page-out')
      expect(motion(`${day}?${group}&event=git%2Fa%2F1`, `${day}?${group}&event=git%2Fa%2F2`)).toBe(
        'none'
      )
    })

    it('moves between the settings home and its sections', () => {
      expect(motion(`${day}?settings=home`, `${day}?settings=logs`)).toBe('page-in')
      expect(motion(`${day}?settings=gitlab%2Foss`, `${day}?settings=home`)).toBe('page-out')
      expect(motion(`${day}?settings=logs`, `${day}?settings=general`)).toBe('none')
    })

    it('opens and leaves the settings in place, as a tab of the bottom bar', () => {
      expect(motion(day, `${day}?settings=home`)).toBe('none')
      expect(motion(day, `${day}?settings=logs`)).toBe('none')
      expect(motion(`${day}?settings=home`, day)).toBe('none')
      expect(motion(`${day}?settings=gitlab%2Foss`, day)).toBe('none')
    })
  })
})

describe('NavigationMotionService', () => {
  function start(): { router: Router; motion: NavigationMotionService } {
    TestBed.configureTestingModule({ providers: [provideAppRouter(), provideLocationMocks()] })
    return { router: TestBed.inject(Router), motion: TestBed.inject(NavigationMotionService) }
  }

  /** The classes a page that comes now gets. */
  function entering(motion: NavigationMotionService, target: MotionTarget = 'page'): string[] {
    const element = document.createElement('section')
    motion.enter({ target: element, animationComplete: () => undefined }, target)
    return [...element.classList]
  }

  it('moves what the navigation being rendered puts in, none at first', async () => {
    const { router, motion } = start()
    router.initialNavigation()
    await router.navigateByUrl('/week/2026-11-17')
    expect(entering(motion, 'view')).toEqual([])

    await router.navigateByUrl('/day/2026-11-17')
    expect(entering(motion, 'view')).toEqual(['view-enter', 'motion-switch'])
    expect(entering(motion, 'page')).toEqual([])
    await router.navigateByUrl('/day/2026-11-17?event=git%2Fa%2F1')
    expect(entering(motion)).toEqual(['page-enter', 'motion-page-in'])
    await router.navigateByUrl('/day/2026-11-17')
    expect(entering(motion)).toEqual(['page-enter', 'motion-page-out'])
  })

  it('keeps still for what comes after its navigation has been rendered', async () => {
    const { router, motion } = start()
    router.initialNavigation()
    await router.navigateByUrl('/day/2026-11-17')
    await router.navigateByUrl('/day/2026-11-17?event=git%2Fa%2F1')

    TestBed.inject(ApplicationRef).tick()

    expect(entering(motion)).toEqual([])
  })

  it('makes what leaves inert at once, and lets it go once its own animation ends', async () => {
    const { router, motion } = start()
    router.initialNavigation()
    await router.navigateByUrl('/day/2026-11-17?event=git%2Fa%2F1')
    await router.navigateByUrl('/day/2026-11-17')
    const page = document.createElement('section')
    const complete = vi.fn()

    motion.leave({ target: page, animationComplete: complete }, 'page')

    expect(page.hasAttribute('inert')).toBe(true)
    expect([...page.classList]).toEqual(['page-leave', 'motion-page-out'])
    // jsdom runs no animation, so there is none to wait for.
    await vi.waitFor(() => expect(complete).toHaveBeenCalledOnce())
  })

  /** Back, as a browser does it: the popstate event, then the router's navigation. */
  async function back(router: Router, browserAnimated: boolean): Promise<void> {
    const event = new PopStateEvent('popstate')
    // jsdom's event has no `hasUAVisualTransition`; Safari sets it after its edge swipe.
    Object.defineProperty(event, 'hasUAVisualTransition', { value: browserAnimated })
    const ended = firstValueFrom(router.events.pipe(filter(e => e instanceof NavigationEnd)))
    window.dispatchEvent(event)
    TestBed.inject(Location).back()
    await ended
  }

  it('keeps still for a Back the browser animated itself, and slides for another', async () => {
    const { router, motion } = start()
    router.initialNavigation()
    const group = '/day/2026-11-17?events=git%2Fa%2F1&events=git%2Fa%2F2'
    await router.navigateByUrl('/day/2026-11-17')
    await router.navigateByUrl(group)
    await router.navigateByUrl(`${group}&event=git%2Fa%2F1`)

    await back(router, true)
    expect(router.url).toBe(group)
    expect(entering(motion)).toEqual([])

    await back(router, false)
    expect(router.url).toBe('/day/2026-11-17')
    expect(entering(motion)).toEqual(['page-enter', 'motion-page-out'])
  })
})
