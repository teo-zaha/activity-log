import { Location } from '@angular/common'
import { DestroyRef, Injector, afterNextRender, inject } from '@angular/core'
import { MatDialog } from '@angular/material/dialog'
import { SettingsDialogLauncher } from '../../components/settings-dialog/settings-dialog-launcher.service'
import { toggleShortcutHelp } from '../../components/shortcut/shortcut-help-dialog.component'
import { NAV_GROUPS } from '../../models/nav'
import { VIEW_SHORTCUTS } from '../../models/shortcuts'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivityExportService } from '../../services/activity-export.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import { ShortcutService } from '../../services/shortcut.service'
import { TopLayer } from './top-layer.service'

/**
 * The app's own commands for its shortcuts, for when no control on screen takes the key: Collect
 * while its button is not shown (the phone's Settings tab), the Timeline's filter from another
 * view, the views, Settings, Back with no layer open, and the help; and what the macOS menu's
 * items without a key run (Export), as the header's buttons do. Bound for as long as
 * the shell lives.
 */
export function bindShellShortcuts(): void {
  const shortcuts = inject(ShortcutService)
  const data = inject(ActivityDataService)
  const viewState = inject(ActivityViewStateService)
  const exports = inject(ActivityExportService)
  const layers = inject(TopLayer)
  const breakpoints = inject(BreakpointState)
  const location = inject(Location)
  const dialogs = inject(MatDialog)
  const injector = inject(Injector)
  const settings = inject(SettingsDialogLauncher)

  /**
   * Focuses the Timeline's filter: at once on the Timeline, where the key is declined when the
   * filter does not take it; from another view after switching there, keeping the range.
   */
  const showTimelineFilter = (): false | void => {
    if (viewState.currentView() === 'list') {
      return shortcuts.handOff('timeline.filter') ? undefined : false
    }
    // What is typed until the filter has focus is the filter's, not a shortcut.
    const cancel = shortcuts.expectHandOff('timeline.filter')
    void viewState.setView('list').then(
      shown => {
        if (shown) afterNextRender(() => shortcuts.handOff('timeline.filter'), { injector })
        else cancel()
      },
      () => cancel()
    )
  }

  const unbind = [
    shortcuts.fallback(
      'app.collect',
      () => (data.collectShownRange() === null ? false : undefined),
      () => !data.isLoading()
    ),
    // Not from under a layer, where the filter is out of reach, nor without sources, where the
    // Timeline has no filter.
    shortcuts.fallback(
      'app.filter',
      showTimelineFilter,
      () => layers.top() === null && !data.hasNoSources()
    ),
    // Not from under a layer, as ⌘F: the view would change behind the details or the settings.
    ...NAV_GROUPS.flatMap(group => group.items).map(item =>
      shortcuts.fallback(
        VIEW_SHORTCUTS[item.id],
        () => void viewState.setView(item.id),
        () => layers.top() === null
      )
    ),
    // Over the details on a wider screen as a dialog; not on a phone, where the Settings tab would
    // open under the details, unseen until they close.
    shortcuts.fallback(
      'app.settings',
      () => settings.open(),
      () => !(breakpoints.phone() && layers.top() === 'details')
    ),
    shortcuts.fallback(
      'app.export-markdown',
      () => void exports.exportShown('markdown'),
      exports.canExport
    ),
    shortcuts.fallback(
      'app.export-json',
      () => void exports.exportShown('json'),
      exports.canExport
    ),
    // What Alt+← did in the desktop app's web view before its browser keys were turned off.
    shortcuts.fallback('app.back', () => location.back()),
    shortcuts.fallback('app.help', () => toggleShortcutHelp(dialogs)),
  ]
  inject(DestroyRef).onDestroy(() => unbind.forEach(off => off()))
}
