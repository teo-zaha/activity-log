import { Location } from '@angular/common'
import { DestroyRef, Injector, afterNextRender, inject } from '@angular/core'
import { MatDialog } from '@angular/material/dialog'
import { toggleShortcutHelp } from '../../components/shortcut/shortcut-help-dialog.component'
import { ActivityService } from '../../services/activity.service'
import { ShortcutService } from '../../services/shortcut.service'
import { TopLayer } from './top-layer.service'

/**
 * The app's own commands for its shortcuts, for when no control on screen takes the key: Collect
 * while its button is not shown (the phone's Settings tab), the Timeline's filter from another
 * view, Back with no layer open, and the help. Bound for as long as the shell lives.
 */
export function bindShellShortcuts(): void {
  const shortcuts = inject(ShortcutService)
  const activity = inject(ActivityService)
  const layers = inject(TopLayer)
  const location = inject(Location)
  const dialogs = inject(MatDialog)
  const injector = inject(Injector)

  /**
   * Focuses the Timeline's filter: at once on the Timeline, where the key is declined when the
   * filter does not take it; from another view after switching there, keeping the range.
   */
  const showTimelineFilter = (): false | void => {
    if (activity.currentView() === 'list') {
      return shortcuts.handOff('timeline.filter') ? undefined : false
    }
    void activity.setView('list').then(shown => {
      if (shown) afterNextRender(() => shortcuts.handOff('timeline.filter'), { injector })
    })
  }

  const unbind = [
    shortcuts.fallback(
      'app.collect',
      () => (activity.collectShownRange() === null ? false : undefined),
      () => !activity.isLoading()
    ),
    // Not from under a layer, where the filter is out of reach, nor without sources, where the
    // Timeline has no filter.
    shortcuts.fallback(
      'app.filter',
      showTimelineFilter,
      () => layers.top() === null && !activity.hasNoSources()
    ),
    // What Alt+← did in the desktop app's web view before its browser keys were turned off.
    shortcuts.fallback('app.back', () => location.back()),
    shortcuts.fallback('app.help', () => toggleShortcutHelp(dialogs)),
  ]
  inject(DestroyRef).onDestroy(() => unbind.forEach(off => off()))
}
