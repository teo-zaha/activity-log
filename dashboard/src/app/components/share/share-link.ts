import { DOCUMENT } from '@angular/common'
import { inject } from '@angular/core'
import { type Params, Router, type UrlTree } from '@angular/router'
import { clipboard } from '@signality/core/browser/clipboard'
import { webShare } from '@signality/core/browser/web-share'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'
import { type Failure, failure, failureThen, failureWhile } from '../../services/failure'
import { ToastService } from '../../services/toast.service'
import { isWebLink } from '../../utils/text-links'

/** Query parameters a shared desktop link may keep: what the view shows, never the settings. */
export type SharedParam = 'q' | 'from' | 'to' | 'event' | 'events'

const DEEP_LINK_BASE = 'activity-log://'

/**
 * What shared links start with: the desktop app's `activity-log://`, or in the browser this
 * page's own origin (the API does not serve the dashboard, so it is never the API's).
 */
export function injectLinkBase(): string {
  return inject(RuntimeEnvironmentService).isTauri()
    ? DEEP_LINK_BASE
    : `${inject(DOCUMENT).location.origin}/`
}

/**
 * The link to the current URL, `activity-log://day/2026-11-17?q=standup` (or the same path on
 * `base`), with only the `keep` parameters of its query, plus `extra`.
 */
export function appLink(
  router: Router,
  keep: readonly SharedParam[],
  extra: Params = {},
  base = DEEP_LINK_BASE
): string {
  const tree = router.parseUrl(router.url)
  const params: Params = {}
  for (const key of keep) {
    const value: unknown = tree.queryParams[key]
    if ((typeof value === 'string' && value) || Array.isArray(value)) params[key] = value
  }
  tree.queryParams = { ...params, ...extra }
  return linkTo(router, tree, base)
}

/** The page's own address in the browser: its path and its whole query, settings included. */
export function pageLink(router: Router, base: string): string {
  return linkTo(router, router.parseUrl(router.url), base)
}

/** `tree` on `base`, without a fragment. */
function linkTo(router: Router, tree: UrlTree, base: string): string {
  tree.fragment = null
  return `${base}${router.serializeUrl(tree).replace(/^\//, '')}`
}

/**
 * Opens the system share sheet with a link, else copies it and says whether that worked, and if
 * not, why. A web link goes in `url`; a deep link goes in `text`, since WebKit's Web Share
 * refuses a custom-scheme `url` such as `activity-log://`.
 */
export function injectShareLink(): (link: string) => Promise<void> {
  const share = webShare()
  const copier = clipboard()
  const toast = inject(ToastService)

  /** Copies `link`; null once it is on the clipboard, else why not. */
  async function copy(link: string): Promise<Failure | null> {
    // Without the Clipboard API `copy` would do nothing.
    if (!copier.isSupported()) {
      return failure('Copying the link failed: this browser gives the page no clipboard')
    }
    try {
      await copier.copy(link)
      return null
    } catch (error) {
      return failureWhile('Copying the link', error)
    }
  }

  return async link => {
    const data: ShareData = isWebLink(link)
      ? { title: 'Activity Log', url: link }
      : { title: 'Activity Log', text: link }
    // Why the share sheet did not take the link, kept for a copy that fails too.
    let refused: Failure | null = null
    if (share.isSupported() && share.canShare(data)) {
      try {
        // Resolves on cancel too; only a failure falls back to copying.
        await share.share(data)
        return
      } catch (error) {
        refused = failureWhile('Sharing the link', error)
      }
    }
    const copied = await copy(link)
    if (copied === null) toast.success('Link copied')
    else toast.failure(refused === null ? copied : failureThen(refused, copied))
  }
}
