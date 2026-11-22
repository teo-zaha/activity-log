import { DOCUMENT } from '@angular/common'
import {
  type AnimationCallbackEvent,
  type EnvironmentProviders,
  EnvironmentInjector,
  Service,
  afterNextRender,
  inject,
  makeEnvironmentProviders,
  provideEnvironmentInitializer,
} from '@angular/core'
import { Router, type UrlTree } from '@angular/router'
import { listener, setupSync } from '@signality/core/browser/listener'
import { routerListener } from '@signality/core/router/router-listener'
import { type AppLocation, isCalendarView, parseAppUrl, sameView } from './utils/app-url'

/**
 * How the screen changes between two URLs; `none` keeps it still. `switch` cross-fades between
 * calendar views. `page-in` and `page-out` move a page of event details or settings (the list,
 * then a section) in or out, which `styles.scss` animates on a phone only, where they are pages.
 */
export type NavigationMotion = 'none' | 'switch' | 'page-in' | 'page-out'

/** What moves: a page of the details or the settings, or a calendar view. */
export type MotionTarget = 'page' | 'view'

/**
 * How many pages deep a URL is where details and settings are pages: event details (a group's
 * list, then one of its events) or settings (the home list, then a section).
 */
function pageDepth(location: AppLocation): number {
  const event = location.event === null ? 0 : 1
  const group = location.events.length > 0 ? 1 : 0
  const settings = location.settings === null ? 0 : location.settings === 'home' ? 1 : 2
  return event + group + settings
}

/**
 * Only switching between calendar views moves, and a page of details or settings going in or
 * out. Opening and leaving the settings is a switch of the phone's bottom-bar tabs, which keeps
 * still as the views' tabs do. Other query changes (filter, range, a step through a group), a
 * calendar view stepping to another day and a switch to or from the timeline keep still: a long
 * timeline would repaint thousands of rows while it fades.
 */
export function navigationMotion(from: UrlTree, to: UrlTree): NavigationMotion {
  const [before, after] = [parseAppUrl(from), parseAppUrl(to)]
  if (before === null || after === null) return 'none'
  const tabSwitch = (before.settings === null) !== (after.settings === null)
  if (sameView(before, after) && !tabSwitch) {
    const [depthBefore, depthAfter] = [pageDepth(before), pageDepth(after)]
    if (depthAfter > depthBefore) return 'page-in'
    if (depthAfter < depthBefore) return 'page-out'
  }
  const calendars = isCalendarView(before.view) && isCalendarView(after.view)
  return calendars && before.view !== after.view ? 'switch' : 'none'
}

/** The motions that move each target. */
function moves(target: MotionTarget, motion: NavigationMotion): boolean {
  return target === 'page' ? motion === 'page-in' || motion === 'page-out' : motion === 'switch'
}

/**
 * Runs the element's own animations (not its subtree's) from the first frame drawn after now, and
 * resolves once they have ended or were cancelled. Their clock would start as they are read, and
 * the render that puts a page in can take longer than its slide on a slow device (B80): the slide
 * would be over before it was ever drawn. Held at their start until the next frame instead, and
 * then resumed only if still held: one cancelled meanwhile (the page left as it came, `idle`) or
 * ended (`finished`) would start again on `play()`.
 */
function runFromNextFrame(element: Element): Promise<unknown> {
  const held = element.getAnimations()
  held.forEach(animation => animation.pause())
  // One frame, once: no Signality composable covers a frame callback.
  requestAnimationFrame(() => {
    for (const animation of held) if (animation.playState === 'paused') animation.play()
  })
  return Promise.allSettled(held.map(animation => animation.finished))
}

/**
 * The motion of the navigation being rendered, for the `animate.enter`/`animate.leave` callbacks
 * of what it puts in or takes out:
 *
 * ```html
 * <section (animate.enter)="motion.enter($event, 'page')" (animate.leave)="motion.leave($event, 'page')">
 * ```
 *
 * A navigation decides its motion before the router activates it, from the URL on screen to the
 * one it goes to, and the motion lasts until that navigation has been rendered. What comes or goes
 * later without a navigation (details whose entry a collection removes or brings back) keeps
 * still. Each element keeps the classes of its own motion (`page-enter motion-page-in`, see
 * `styles.scss`) until its animation ends, so a slide that runs on finishes whatever comes next.
 *
 * A Back the browser animated itself (Safari's edge swipe: the popstate event says
 * `hasUAVisualTransition`) keeps still, so the page does not slide a second time.
 */
@Service()
export class NavigationMotionService {
  private readonly document = inject(DOCUMENT)
  private readonly injector = inject(EnvironmentInjector)
  private current: { readonly navigation: number; readonly motion: NavigationMotion } | null = null
  private browserAnimated = false

  /**
   * Starts following the router, in an injection context (`provideNavigationMotion`). Only this
   * reads the router, so a component that injects the service for its callbacks creates none.
   */
  follow(router = inject(Router)): void {
    // At once: a popstate must be seen before the navigation it starts reaches its guards.
    setupSync(() =>
      listener(this.document.defaultView ?? window, 'popstate', event => {
        this.browserAnimated = event.hasUAVisualTransition === true
      })
    )
    // Every navigation checks its guards before the router activates it; only some resolve.
    routerListener('guardscheckend', event => {
      const swiped = this.browserAnimated && router.currentNavigation()?.trigger === 'popstate'
      this.browserAnimated = false
      if (!event.shouldActivate) return
      // The URL on screen: the last navigation that succeeded, until this one activates.
      const shown = router.lastSuccessfulNavigation()?.finalUrl
      const motion =
        shown && !swiped
          ? navigationMotion(shown, router.parseUrl(event.urlAfterRedirects))
          : 'none'
      this.current = { navigation: event.id, motion }
    })
    // The navigation's own render puts in and takes out what it moves (the callbacks run after
    // it, before its `read` phase); then it is over.
    routerListener('navigationend', event =>
      afterNextRender({ read: () => this.end(event.id) }, { injector: this.injector })
    )
    routerListener('navigationcancel', event => this.end(event.id))
    routerListener('navigationerror', event => this.end(event.id))
  }

  /** `(animate.enter)`: what comes slides or fades in when its navigation moves it. */
  enter(event: AnimationCallbackEvent, target: MotionTarget): void {
    const motion = this.motionOf(target)
    if (motion === null) return
    const element = event.target
    const classes = [`${target}-enter`, `motion-${motion}`]
    element.classList.add(...classes)
    void runFromNextFrame(element).then(() => element.classList.remove(...classes))
  }

  /**
   * `(animate.leave)`: what goes is `inert` at once, out of focus, the keyboard and the
   * accessibility tree, so nothing in it is found twice, and leaves as its navigation moves it.
   * If it held focus, focus goes to what holds it (the main content for a view, the details or the
   * settings for a page), never to the page behind, also when it goes without a navigation; the
   * component of a page then focuses the title of the page that comes.
   */
  leave(event: AnimationCallbackEvent, target: MotionTarget): void {
    const element = event.target
    const hadFocus = element.contains(this.document.activeElement)
    element.toggleAttribute('inert', true)
    if (hadFocus) {
      element.parentElement?.closest<HTMLElement>('[tabindex]')?.focus({ preventScroll: true })
    }
    const motion = this.motionOf(target)
    if (motion === null) {
      event.animationComplete()
      return
    }
    element.classList.add(`${target}-leave`, `motion-${motion}`)
    void runFromNextFrame(element).then(() => event.animationComplete())
  }

  private motionOf(target: MotionTarget): NavigationMotion | null {
    const motion = this.current?.motion ?? 'none'
    return moves(target, motion) ? motion : null
  }

  private end(navigation: number): void {
    if (this.current?.navigation === navigation) this.current = null
  }
}

/** Has `NavigationMotionService` follow the router from the app's start. */
export const provideNavigationMotion = (): EnvironmentProviders =>
  makeEnvironmentProviders([
    provideEnvironmentInitializer(() => inject(NavigationMotionService).follow()),
  ])
