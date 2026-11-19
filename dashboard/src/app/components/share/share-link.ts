import { inject } from '@angular/core'
import { type Params, Router } from '@angular/router'
import { clipboard } from '@signality/core/browser/clipboard'
import { webShare } from '@signality/core/browser/web-share'
import { ToastService } from '../../services/toast.service'

/** Query parameters a shared link may keep: what the view shows, never the settings dialog. */
export type SharedParam = 'q' | 'from' | 'to' | 'event' | 'events'

/**
 * The deep link to the current URL, `activity-log://day/2026-11-17?q=standup`, with only the
 * `keep` parameters of its query, plus `extra`.
 */
export function appLink(router: Router, keep: readonly SharedParam[], extra: Params = {}): string {
  const tree = router.parseUrl(router.url)
  const params: Params = {}
  for (const key of keep) {
    const value: unknown = tree.queryParams[key]
    if ((typeof value === 'string' && value) || Array.isArray(value)) params[key] = value
  }
  tree.queryParams = { ...params, ...extra }
  tree.fragment = null
  return `activity-log://${router.serializeUrl(tree).replace(/^\//, '')}`
}

/**
 * Opens the system share sheet with a link, else copies it and says so. The link goes in `text`:
 * WebKit's Web Share refuses a custom-scheme `url` such as `activity-log://`.
 */
export function injectShareLink(): (link: string) => Promise<void> {
  const share = webShare()
  const copier = clipboard()
  const toast = inject(ToastService)
  return async link => {
    const data = { title: 'Activity Log', text: link }
    if (share.isSupported() && share.canShare(data)) {
      try {
        // Resolves on cancel too; only a failure falls back to copying.
        await share.share(data)
        return
      } catch {
        // The system refused the link: copy it instead.
      }
    }
    await copier.copy(link)
    toast.success('Link copied')
  }
}
