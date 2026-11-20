import { Injector, Service, afterNextRender, computed, effect, inject } from '@angular/core'
import { SettingsDialogLauncher } from '../../components/settings-dialog/settings-dialog-launcher.service'
import { ActivityService } from '../../services/activity.service'

/**
 * The layers over the views, top first. The settings dialog (wider screens) is a Material dialog
 * over everything, the details included, with its own focus and Escape. The details cover
 * whatever shows, the phone's Settings tab included.
 */
const LAYERS = ['settingsDialog', 'details', 'settingsPage'] as const
export type ShellLayer = (typeof LAYERS)[number]

/**
 * Which layer is on top: the one Escape closes and focus stays in. A layer under it takes
 * neither; the shell makes everything under the details `inert`.
 */
@Service()
export class TopLayer {
  private readonly activity = inject(ActivityService)
  private readonly settings = inject(SettingsDialogLauncher)

  readonly top = computed<ShellLayer | null>(() => {
    if (this.settings.dialogOpen()) return 'settingsDialog'
    if (this.activity.detailsOpen()) return 'details'
    if (this.settings.pageOpen()) return 'settingsPage'
    return null
  })

  /**
   * Calls `exposed` after the render in which `layer` is on top again because a layer over it
   * closed, so the layer can take the focus the closed one had, which would otherwise fall to
   * the page.
   */
  whenExposed(layer: ShellLayer, exposed: () => void, injector = inject(Injector)): void {
    let previous: ShellLayer | null = null
    effect(
      () => {
        const top = this.top()
        const uncovered =
          top === layer && previous !== null && LAYERS.indexOf(previous) < LAYERS.indexOf(layer)
        if (uncovered) afterNextRender(exposed, { injector })
        previous = top
      },
      { injector }
    )
  }
}
