import { Service, computed, type Signal } from '@angular/core'
import { breakpoints } from '@signality/core/browser/breakpoints'
import { downQuery } from '../utils/breakpoints'

/**
 * The window's width as the named breakpoints of `_dashboard-shared.scss` cut it, one state for
 * the whole dashboard, so no two parts disagree: the bottom bar on a phone, the rail of icons
 * above it up to a tablet, the sidebar above that.
 */
@Service()
export class BreakpointState {
  private readonly matches = breakpoints({
    phone: downQuery('phone'),
    tablet: downQuery('tablet'),
    desktopCompact: downQuery('desktop-compact'),
  })

  /** At or below each named width (`ui.down(name)` in SCSS). */
  readonly phone: Signal<boolean> = this.matches.phone
  readonly tablet: Signal<boolean> = this.matches.tablet
  /** Slice 9's switch between the docked details and the flyout. */
  readonly desktopCompact: Signal<boolean> = this.matches.desktopCompact
  /** The rail: at or below a tablet's width, above a phone's. */
  readonly rail: Signal<boolean> = computed(() => this.tablet() && !this.phone())
}
