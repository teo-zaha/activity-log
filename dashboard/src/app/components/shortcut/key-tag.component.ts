import { Component, computed, inject, input } from '@angular/core'
import type { ShortcutId } from '../../models/shortcuts'
import { ShortcutService } from '../../services/shortcut.service'

/**
 * The key tag of a shortcut (`⌘R`, `/`), from the registry, for text buttons and fields that have
 * room for one. Not read out: the control names its keys in `aria-keyshortcuts`. Not shown where
 * the shortcut has no key here (a desktop key in the browser) or in the phone layout.
 */
@Component({
  selector: 'app-key-tag',
  template: `{{ hint() }}`,
  styleUrl: './key-tag.component.scss',
  host: { 'aria-hidden': 'true', '[hidden]': 'hint() === null' },
})
export class KeyTagComponent {
  readonly shortcut = input.required<ShortcutId>()
  private readonly shortcuts = inject(ShortcutService)
  protected readonly hint = computed(() => this.shortcuts.hint(this.shortcut()))
}
