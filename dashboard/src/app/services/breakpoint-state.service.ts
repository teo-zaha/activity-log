import { Service, computed } from '@angular/core'
import { mediaQuery } from '@signality/core/browser/media-query'
import { downQuery } from '../utils/breakpoints'

/**
 * The navigation's layout as the window's width decides it (`_dashboard-shared.scss`): the bottom
 * bar on a phone, the rail of icons above it up to a tablet, the sidebar above that. One state for
 * every part of the navigation, so they never disagree.
 */
@Service()
export class BreakpointState {
  readonly phone = mediaQuery(downQuery('phone'))
  private readonly tablet = mediaQuery(downQuery('tablet'))
  /** The rail: at or below a tablet's width, above a phone's. */
  readonly rail = computed(() => this.tablet() && !this.phone())
}
