import { Service, inject } from '@angular/core'
import { Router, UrlSerializer, type UrlTree } from '@angular/router'
import {
  DEFAULT_LOCATION,
  dayOf,
  deepLinkPath,
  parseAppUrl,
  queryParamsOf,
  sameView,
  viewCommands,
} from '../utils/app-url'
import { todayKey } from '../utils/local-day'
import { ActivityDataService } from './activity-data.service'

/**
 * The one way in for links from outside the dashboard, such as a desktop deep link
 * (`activity-log://day/2026-11-17?q=review`): they become a navigation to the route they name.
 */
@Service()
export class DeepLinkService {
  private readonly router = inject(Router)
  private readonly serializer = inject(UrlSerializer)
  private readonly data = inject(ActivityDataService)

  /**
   * Shows what `link` names: an `activity-log:` URL or an app path. A link to no view is ignored
   * (false). Only the query parameters the dashboard knows are kept; a link with a range collects
   * it, as opening the app on that URL would. A link to the view on screen, opening details or
   * settings over it, leaves it where it is scrolled to; another view shows from its start.
   */
  async open(link: string): Promise<boolean> {
    const target = this.resolve(link)
    if (target === null) return false
    const location = parseAppUrl(target) ?? DEFAULT_LOCATION
    const shown = parseAppUrl(this.router.parseUrl(this.router.url))
    const navigated = await this.router.navigateByUrl(target, {
      scroll: shown !== null && sameView(shown, location) ? 'manual' : 'after-transition',
    })
    const { from, to } = location
    if (navigated && from && to) void this.data.collect(from, to)
    return navigated
  }

  /** The URL `link` stands for, built anew from what it validly says; null for no view. */
  resolve(link: string): UrlTree | null {
    const path = deepLinkPath(link)
    if (path === null) return null
    let parsed: UrlTree
    try {
      parsed = this.serializer.parse(path)
    } catch {
      return null
    }
    // A link is untrusted; a range the backend would refuse is read as none (`parseAppUrl`).
    const location = parseAppUrl(parsed)
    if (location === null) return null
    return this.router.createUrlTree(viewCommands(location.view, dayOf(location) ?? todayKey()), {
      queryParams: queryParamsOf(location),
      queryParamsHandling: 'replace',
    })
  }
}
