import { Service, type TemplateRef, signal } from '@angular/core'

/**
 * Actions a settings section puts in the page header, such as the logs' Copy and Clear. Each
 * settings view provides its own; a section without one keeps its actions in place.
 */
@Service({ autoProvided: false })
export class SettingsPageActions {
  readonly template = signal<TemplateRef<unknown> | null>(null)
}
