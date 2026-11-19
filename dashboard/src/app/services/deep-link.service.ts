import { Service, inject } from '@angular/core'
import { Router, UrlSerializer, type UrlTree } from '@angular/router'
import { deepLinkPath, parseAppUrl, queryParamsOf, viewCommands } from '../utils/app-url'
import { dateOfKey, todayKey } from '../utils/local-day'
import { ActivityService } from './activity.service'

/**
 * The one way in for links from outside the dashboard, such as a desktop deep link
 * (`activity-log://day/2026-11-17?q=review`): they become a navigation to the route they name.
 */
@Service()
export class DeepLinkService {
  private readonly router = inject(Router)
  private readonly serializer = inject(UrlSerializer)
  private readonly activity = inject(ActivityService)

  /**
   * Shows what `link` names: an `activity-log:` URL or an app path. A link to no view is ignored
   * (false). Only the query parameters the dashboard knows are kept; a link with a range collects
   * it, as opening the app on that URL would.
   */
  async open(link: string): Promise<boolean> {
    const target = this.resolve(link)
    if (target === null) return false
    const navigated = await this.router.navigateByUrl(target)
    const { from, to } = target.queryParams
    if (navigated && typeof from === 'string' && typeof to === 'string') {
      void this.activity.collect(from, to)
    }
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
    const found = parseAppUrl(parsed)
    if (found === null) return null
    const location = collectable(found.from, found.to) ? found : { ...found, from: null, to: null }
    return this.router.createUrlTree(viewCommands(location.view, location.day ?? todayKey()), {
      queryParams: queryParamsOf(location),
      queryParamsHandling: 'replace',
    })
  }
}

/** The longest range the backend collects (`CollectOptions::MAX_DAYS`, ten years). */
const MAX_COLLECT_DAYS = 3_653

/**
 * Whether a link's range is one the backend would collect; a link is untrusted, so a backwards
 * or longer range is dropped instead of being sent. No range at all passes.
 */
function collectable(from: string | null, to: string | null): boolean {
  if (from === null || to === null) return true
  const days = Math.round((dateOfKey(to).getTime() - dateOfKey(from).getTime()) / 86_400_000) + 1
  return days >= 1 && days <= MAX_COLLECT_DAYS
}
