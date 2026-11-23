import { Component, computed, inject, input } from '@angular/core'
import type { ShortcutId } from '../../models/shortcuts'
import { ShortcutService } from '../../services/shortcut.service'

/**
 * The key tag of a shortcut (`⌘R`, `/`), from the registry: a small badge over its control's
 * corner, shown while Shift is held on its own (`ShortcutService.revealKeys`), so it takes no
 * room and moves nothing. Its control needs `position: relative`. Not read out: the control
 * names its keys in `aria-keyshortcuts`. Nothing where the shortcut has no key here (a desktop
 * key in the browser, a single key turned off).
 *
 * `resting: 'shift'` shows Shift while the keys are not revealed, on the one control that says
 * holding it shows them (the sidebar's Keyboard shortcuts).
 */
@Component({
  selector: 'app-key-tag',
  template: `{{ text() }}`,
  styleUrl: './key-tag.component.scss',
  host: {
    'aria-hidden': 'true',
    '[hidden]': 'text() === null',
    '[class.is-shown]': 'shown()',
  },
})
export class KeyTagComponent {
  readonly shortcut = input.required<ShortcutId>()
  readonly resting = input<'shift' | null>(null)

  private readonly shortcuts = inject(ShortcutService)
  private readonly hint = computed(() => this.shortcuts.hint(this.shortcut()))
  private readonly revealed = computed(() => this.shortcuts.revealKeys() && this.hint() !== null)

  protected readonly shown = computed(() => this.revealed() || this.resting() !== null)
  protected readonly text = computed(() =>
    this.resting() === 'shift' && !this.revealed() ? this.shortcuts.shiftKey : this.hint()
  )
}
