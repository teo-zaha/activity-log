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
import { ViewUnavailableComponent } from './components/view-unavailable/view-unavailable.component'
import { VIEW_LABELS } from './models/nav'
import { ShellLayoutComponent } from './layouts/shell/shell-layout.component'
import { ViewsLayoutComponent } from './layouts/views/views-layout.component'
import { provideNavigationMotion } from './navigation-motion'
import { ActivityService } from './services/activity.service'
import { failureWhile } from './services/failure'
import { type CalendarView, VIEW_PATHS, viewCommands } from './utils/app-url'
import { isDayKey, longDayLabel, todayKey } from './utils/local-day'

const APP_TITLE = 'Activity Log'

/**
 * A view's code, loaded when the view first opens. When it does not load for the app's first
 * navigation (an app update, a dev server that went away), the view's place shows
 * `ViewUnavailableComponent`, which offers a reload, so the shell and the navigation come up
 * instead of nothing; the failure is a notice. A later navigation fails as before: the view on
 * screen stays and a notice says so (`provideAppRouter`).
 */
export function loadView(load: () => Promise<Type<unknown>>): () => Promise<Type<unknown>> {
  return () => {
    const router = inject(Router)
    const activity = inject(ActivityService)
    return load().catch((error: unknown) => {
      if (router.navigated) throw error
      activity.notice.set({ origin: 'view', failure: failureWhile('Opening this view', error) })
      return ViewUnavailableComponent
    })
  }
}

const loadWeekView = loadView(() =>
  import('./components/week-view/week-view.component').then(m => m.WeekViewComponent)
)

/** The calendar views match only a real day; any other second segment falls through. */
const isDayParam: CanMatchFn = (_route, segments) => isDayKey(segments[1]?.path ?? '')

/** Redirects keep the query, so `/?settings=logs` still opens the settings over the timeline. */
function redirectKeepingQuery(commands: () => string[]): RedirectFunction {
  return ({ queryParams }) =>
    inject(Router).createUrlTree(commands(), { queryParams, queryParamsHandling: 'replace' })
}

const showTimeline = (): string[] => viewCommands('list', todayKey())

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

/**
 * The shell (header, navigation, main content) around every page, and in it the views' layout
 * (range, Collect, source chips) around the activity views. A page that needs neither range nor
 * chips is the views layout's sibling under the shell.
 */
export const APP_ROUTES: Routes = [
  // `/` shows the timeline; the shell's empty path would otherwise match it with no page in it.
  { path: '', pathMatch: 'full', redirectTo: redirectKeepingQuery(showTimeline) },
  {
    path: '',
    component: ShellLayoutComponent,
    children: [
      {
        path: '',
        component: ViewsLayoutComponent,
        children: [
          {
            path: VIEW_PATHS.list,
            loadComponent: loadView(() =>
              import('./components/timeline/timeline.component').then(m => m.TimelineComponent)
            ),
            title: VIEW_LABELS.list,
          },
          ...calendarRoutes(
            'day',
            loadView(() =>
              import('./components/day-view/day-view.component').then(m => m.DayViewComponent)
            )
          ),
          ...calendarRoutes('workweek', loadWeekView),
          ...calendarRoutes('week', loadWeekView),
        ],
      },
    ],
  },
  // Unknown paths show the timeline: a path no page under the shell matches comes here.
  { path: '**', redirectTo: redirectKeepingQuery(showTimeline) },
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
    provideNavigationMotion(),
  ]
}
