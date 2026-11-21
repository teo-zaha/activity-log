import { Location } from '@angular/common'
import { DestroyRef, Injector, afterNextRender, inject } from '@angular/core'
import { MatDialog } from '@angular/material/dialog'
import { SettingsDialogLauncher } from '../../components/settings-dialog/settings-dialog-launcher.service'
import { injectCopyLink, injectViewLink } from '../../components/share/share-link'
import { toggleShortcutHelp } from '../../components/shortcut/shortcut-help-dialog.component'
import { NAV_GROUPS } from '../../models/nav'
import { VIEW_SHORTCUTS } from '../../models/shortcuts'
import { ActivityService } from '../../services/activity.service'
import { ShortcutService } from '../../services/shortcut.service'
import { TopLayer } from './top-layer.service'

/**
 * The app's own commands for its shortcuts, for when no control on screen takes the key: Collect
 * while its button is not shown (the phone's Settings tab), the Timeline's filter from another
 * view, the views, Settings, Back with no layer open, and the help; and what the macOS menu's
 * items without a key run (Export, Copy Link), as the header's buttons do. Bound for as long as
 * the shell lives.
 */
export function bindShellShortcuts(): void {
  const shortcuts = inject(ShortcutService)
  const activity = inject(ActivityService)
  const layers = inject(TopLayer)
  const location = inject(Location)
  const dialogs = inject(MatDialog)
  const injector = inject(Injector)
  const settings = inject(SettingsDialogLauncher)
  const copyLink = injectCopyLink()
  const viewLink = injectViewLink()

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
    // Not from under a layer, as ⌘F: the view would change behind the details or the settings.
    ...NAV_GROUPS.flatMap(group => group.items).map(item =>
      shortcuts.fallback(
        VIEW_SHORTCUTS[item.id],
        () => void activity.setView(item.id),
        () => layers.top() === null
      )
    ),
    shortcuts.fallback('app.settings', () => settings.open()),
    shortcuts.fallback(
      'app.export-markdown',
      () => void activity.exportMarkdown(),
      activity.canExport
    ),
    shortcuts.fallback('app.export-json', () => void activity.exportJson(), activity.canExport),
    shortcuts.fallback('app.copy-link', () => void copyLink(viewLink())),
    // What Alt+← did in the desktop app's web view before its browser keys were turned off.
    shortcuts.fallback('app.back', () => location.back()),
    shortcuts.fallback('app.help', () => toggleShortcutHelp(dialogs)),
  ]
  inject(DestroyRef).onDestroy(() => unbind.forEach(off => off()))
}
