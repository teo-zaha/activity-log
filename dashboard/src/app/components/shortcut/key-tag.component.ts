import { Component, computed, inject, input } from '@angular/core'
import type { ShortcutId } from '../../models/shortcuts'
import { ShortcutService } from '../../services/shortcut.service'

/**
 * The key tag of a shortcut (`⌘R`, `/`), from the registry: a small badge over its control's
 * corner, shown while Shift is held on its own (`ShortcutService.revealKeys`) and the key would
 * run now (`available`, the rule the keys and the menu follow), so it takes no room and moves
 * nothing. Its control needs `position: relative`. Not read out: the control names its keys in
 * `aria-keyshortcuts`. Nothing where the shortcut has no key here (a desktop key in the browser,
 * a single key turned off).
 *
 * `place`: `inside` the control's top right corner, or `overhang` its bottom right corner, half
 * out, on a compact control whose icon or label fills it.
 *
 * `resting: 'shift'` shows Shift while the keys are not revealed, on the one control that says
 * holding it shows them (the sidebar's Keyboard shortcuts), and only where holding it can show
 * one (`revealable`); never where the pointer cannot hover, a touch screen without a keyboard.
 */
@Component({
  selector: 'app-key-tag',
  template: `{{ text() }}`,
  styleUrl: './key-tag.component.scss',
  host: {
    'aria-hidden': 'true',
    '[hidden]': 'text() === null',
    '[class.is-shown]': 'revealed() || atRest()',
    '[class.is-resting]': 'atRest()',
    '[class.is-overhang]': "place() === 'overhang'",
  },
})
export class KeyTagComponent {
  readonly shortcut = input.required<ShortcutId>()
  readonly place = input<'inside' | 'overhang'>('inside')
  readonly resting = input<'shift' | null>(null)

  private readonly shortcuts = inject(ShortcutService)
  private readonly hint = computed(() => this.shortcuts.hint(this.shortcut()))

  protected readonly revealed = computed(
    () =>
      this.shortcuts.revealKeys() &&
      this.hint() !== null &&
      this.shortcuts.available(this.shortcut())
  )
  /** Shift at rest: shown while the keys are not. */
  protected readonly atRest = computed(
    () => this.resting() === 'shift' && this.shortcuts.revealable() && !this.revealed()
  )
  protected readonly text = computed(() => (this.atRest() ? this.shortcuts.shiftKey : this.hint()))
}
