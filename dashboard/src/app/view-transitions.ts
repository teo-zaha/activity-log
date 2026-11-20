import { DOCUMENT } from '@angular/common'
import {
  type EnvironmentProviders,
  inject,
  makeEnvironmentProviders,
  provideEnvironmentInitializer,
} from '@angular/core'
import {
  Router,
  type UrlTree,
  type ViewTransitionsFeature,
  withViewTransitions,
} from '@angular/router'
import { type AppLocation, dayOf, parseAppUrl } from './utils/app-url'
import { downQuery } from './utils/breakpoints'

/**
 * How the screen changes between two URLs; `none` keeps it still. `page-in` and `page-out` move
 * the event details, or the settings' list and a section, on a phone, where they are pages.
 */
export type ViewTransitionKind = 'none' | 'switch' | 'page-in' | 'page-out'

/**
 * How many pages deep a URL is on a phone: event details (a group's list, then one of its events)
 * or settings (the home list, then a section).
 */
function pageDepth(location: AppLocation): number {
  const event = location.event === null ? 0 : 1
  const group = location.events.length > 0 ? 1 : 0
  const settings = location.settings === null ? 0 : location.settings === 'home' ? 1 : 2
  return event + group + settings
}

/**
 * Only switching between calendar views animates, and on a phone a page of details or settings
 * sliding in or out. Opening and leaving the settings on a phone is a switch of bottom-bar tabs,
 * which keeps still as the timeline's does. Other query changes (filter, range, a step through a group) and a calendar
 * view stepping to another day keep still, and so does a switch to or from the timeline: the
 * transition draws the whole list into its snapshot, which in WebKit takes longer than the
 * switch itself on a long range.
 */
export function viewTransitionKind(from: UrlTree, to: UrlTree, phone = false): ViewTransitionKind {
  const [before, after] = [parseAppUrl(from), parseAppUrl(to)]
  if (before === null || after === null) return 'none'
  const sameView = before.view === after.view && dayOf(before) === dayOf(after)
  const tabSwitch = (before.settings === null) !== (after.settings === null)
  if (phone && sameView && !tabSwitch) {
    const [depthBefore, depthAfter] = [pageDepth(before), pageDepth(after)]
    if (depthAfter > depthBefore) return 'page-in'
    if (depthAfter < depthBefore) return 'page-out'
  }
  if (before.view === after.view || before.view === 'list' || after.view === 'list') return 'none'
  return 'switch'
}

/**
 * What `startViewTransition` answers when no transition runs: the update, done at once. `ready`
 * resolves rather than rejecting as a skipped transition's does, which the router would log.
 */
function withoutTransition(update: (() => unknown) | undefined): ViewTransition {
  const updateCallbackDone = Promise.resolve().then(async () => {
    await update?.()
  })
  return {
    updateCallbackDone,
    ready: updateCallbackDone,
    finished: updateCallbackDone,
    types: new Set<string>(),
    skipTransition: () => undefined,
  }
}

/**
 * Angular's router starts a view transition for every navigation and has no hook to decide
 * before it starts; starting one and skipping it still costs WebKit about 0.2 s. So the
 * document's `startViewTransition` asks `viewTransitionKind` first, from the URL on screen to the
 * one the router is going to, and only starts the transitions that animate. It also sets
 * `data-view-transition` on <html>, which `styles.scss` picks the animation from.
 */
function gateViewTransitions(): void {
  const document = inject(DOCUMENT)
  const router = inject(Router)
  const start = document.startViewTransition?.bind(document)
  if (!start) return

  /** The transition `data-view-transition` belongs to: one that starts replaces the one before. */
  let latest: ViewTransition | null = null
  document.startViewTransition = (
    update?: ViewTransitionUpdateCallback | StartViewTransitionOptions
  ): ViewTransition => {
    const callback = typeof update === 'function' ? update : update?.update
    const navigation = router.currentNavigation()
    const target = navigation?.finalUrl ?? navigation?.extractedUrl
    // One-off reads when the transition would start; `matchMedia` is the plain way to ask once.
    const media = (query: string): boolean => window.matchMedia?.(query).matches ?? false
    const kind =
      target && !media('(prefers-reduced-motion: reduce)')
        ? viewTransitionKind(router.parseUrl(router.url), target, media(downQuery('phone')))
        : 'none'
    if (kind === 'none') return withoutTransition(callback ?? undefined)

    const root = document.documentElement
    root.dataset['viewTransition'] = kind
    const transition = start(update)
    latest = transition
    void transition.finished
      // It rejects when the update does, which the router reports itself.
      .catch(() => undefined)
      .finally(() => {
        if (latest !== transition) return
        latest = null
        delete root.dataset['viewTransition']
      })
    return transition
  }
}

/**
 * Animated view changes where the browser has view transitions (WKWebView, WebView2, Chromium);
 * elsewhere, and in tests, the router navigates without them.
 */
export const withAppViewTransitions = (): ViewTransitionsFeature =>
  withViewTransitions({ skipInitialTransition: true })

/** Starts only the transitions that animate; see `gateViewTransitions`. */
export const provideViewTransitionGate = (): EnvironmentProviders =>
  makeEnvironmentProviders([provideEnvironmentInitializer(gateViewTransitions)])
