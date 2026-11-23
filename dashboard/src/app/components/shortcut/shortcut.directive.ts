import { Directive, ElementRef, computed, effect, inject, input } from '@angular/core'
import type { ShortcutId } from '../../models/shortcuts'
import { ShortcutService } from '../../services/shortcut.service'

/**
 * Binds a shortcut to its control: the key clicks the host, or focuses it if it is a field, while
 * the host is in the page, not `inert` and not disabled; otherwise the key goes to the binding
 * before. Names the keys in `aria-keyshortcuts`; an `<app-key-tag>` inside the control shows them
 * while Shift is held. A host inside a view the router keeps while away binds only that view's
 * entries.
 */
@Directive({
  selector: '[appShortcut]',
  host: { '[attr.aria-keyshortcuts]': 'ariaKeys()' },
})
export class ShortcutDirective {
  /** Null binds nothing, for a control that has a shortcut in one use only. */
  readonly appShortcut = input.required<ShortcutId | null>()

  private readonly shortcuts = inject(ShortcutService)
  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
  protected readonly ariaKeys = computed(() => {
    const id = this.appShortcut()
    return id === null ? null : this.shortcuts.ariaKeys(id)
  })

  constructor() {
    effect(onCleanup => {
      const id = this.appShortcut()
      if (id !== null)
        onCleanup(
          this.shortcuts.bind(
            id,
            () => this.press(),
            () => this.usable()
          )
        )
    })
  }

  private usable(): boolean {
    return (
      this.host.isConnected &&
      this.host.closest('[inert]') === null &&
      !this.host.matches(':disabled')
    )
  }

  private press(): void {
    const field =
      this.host instanceof HTMLInputElement ||
      this.host instanceof HTMLTextAreaElement ||
      this.host instanceof HTMLSelectElement
    if (field) this.host.focus()
    else this.host.click()
  }
}
