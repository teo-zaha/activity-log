import { type EnvironmentProviders, inject, type Provider, Service, type Type } from '@angular/core'
import { Title } from '@angular/platform-browser'
import {
  type ActivatedRouteSnapshot,
  BaseRouteReuseStrategy,
  type CanMatchFn,
  type DetachedRouteHandle,
  provideRouter,
  type RedirectFunction,
  type ResolveFn,
  RouteReuseStrategy,
  Router,
  type RouterStateSnapshot,
  type Routes,
  TitleStrategy,
  withNavigationErrorHandler,
  withRouterConfig,
} from '@angular/router'
import { VIEW_LABELS } from './models/view-option'
import { provideViewTransitionGate, withAppViewTransitions } from './view-transitions'
import { ActivityService } from './services/activity.service'
import { failureWhile } from './services/failure'
import { type CalendarView, VIEW_PATHS, viewCommands } from './utils/app-url'
import { isDayKey, longDayLabel, todayKey } from './utils/local-day'

const APP_TITLE = 'Activity Log'

const loadWeekView = (): Promise<Type<unknown>> =>
  import('./components/week-view/week-view.component').then(m => m.WeekViewComponent)

/** The calendar views match only a real day; any other second segment falls through. */
const isDayParam: CanMatchFn = (_route, segments) => isDayKey(segments[1]?.path ?? '')

/** Redirects keep the query, so `/?settings=logs` still opens the settings over the timeline. */
function redirectKeepingQuery(commands: () => string[]): RedirectFunction {
  return ({ queryParams }) =>
    inject(Router).createUrlTree(commands(), { queryParams, queryParamsHandling: 'replace' })
}

function dayTitle(view: CalendarView): ResolveFn<string> {
  return route => {
    const date = route.paramMap.get('date')
    return `${VIEW_LABELS[view]} · ${longDayLabel(date !== null && isDayKey(date) ? date : todayKey())}`
  }
}

function calendarRoutes(view: CalendarView, loadComponent: () => Promise<Type<unknown>>): Routes {
  const path = VIEW_PATHS[view]
  return [
    { path: `${path}/:date`, canMatch: [isDayParam], loadComponent, title: dayTitle(view) },
    // A missing or invalid day shows today.
    { path, redirectTo: redirectKeepingQuery(() => viewCommands(view, todayKey())) },
  ]
}

export const APP_ROUTES: Routes = [
  {
    path: VIEW_PATHS.list,
    loadComponent: () =>
      import('./components/timeline/timeline.component').then(m => m.TimelineComponent),
    title: VIEW_LABELS.list,
  },
  ...calendarRoutes('day', () =>
    import('./components/day-view/day-view.component').then(m => m.DayViewComponent)
  ),
  ...calendarRoutes('workweek', loadWeekView),
  ...calendarRoutes('week', loadWeekView),
  // `/` and unknown paths show the timeline.
  { path: '**', redirectTo: redirectKeepingQuery(() => viewCommands('list', todayKey())) },
]

/** `Day · Tuesday, 17 November 2026 · Activity Log`. */
@Service({ autoProvided: false })
export class AppTitleStrategy extends TitleStrategy {
  private readonly title = inject(Title)

  override updateTitle(snapshot: RouterStateSnapshot): void {
    const title = this.buildTitle(snapshot)
    this.title.setTitle(title ? `${title} · ${APP_TITLE}` : APP_TITLE)
  }
}

/** By path: the router works on copies of the route objects. */
function isTimeline(route: ActivatedRouteSnapshot): boolean {
  return route.routeConfig?.path === VIEW_PATHS.list
}

/**
 * Keeps the timeline while a calendar view shows and puts it back on return: a long range has
 * thousands of rows, too many to rebuild on every switch. It follows the data while away, as a
 * view of signals does when it is attached again. The calendar views are cheap and not kept.
 */
@Service({ autoProvided: false })
export class TimelineReuseStrategy extends BaseRouteReuseStrategy {
  private timeline: DetachedRouteHandle | null = null

  override shouldDetach(route: ActivatedRouteSnapshot): boolean {
    return isTimeline(route)
  }

  override store(route: ActivatedRouteSnapshot, handle: DetachedRouteHandle | null): void {
    if (isTimeline(route)) this.timeline = handle
  }

  override shouldAttach(route: ActivatedRouteSnapshot): boolean {
    return isTimeline(route) && this.timeline !== null
  }

  override retrieve(route: ActivatedRouteSnapshot): DetachedRouteHandle | null {
    return isTimeline(route) ? this.timeline : null
  }
}

/**
 * The router as the app runs it. Navigations keep the query unless they name a parameter, so the
 * range and the settings stay when the view or the day changes.
 */
export function provideAppRouter(): (Provider | EnvironmentProviders)[] {
  return [
    provideRouter(
      APP_ROUTES,
      withRouterConfig({ defaultQueryParamsHandling: 'merge' }),
      withAppViewTransitions(),
      // A view's code failed to load (an app update, a dropped dev server): say so and stay.
      withNavigationErrorHandler(error =>
        inject(ActivityService).notice.set({
          origin: 'view',
          failure: failureWhile('Opening this view', error.error),
        })
      )
    ),
    { provide: TitleStrategy, useClass: AppTitleStrategy },
    { provide: RouteReuseStrategy, useClass: TimelineReuseStrategy },
    provideViewTransitionGate(),
  ]
}
