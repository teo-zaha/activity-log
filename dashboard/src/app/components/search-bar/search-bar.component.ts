import { Component, input, model } from '@angular/core'
import { form, FormField } from '@angular/forms/signals'
import { MatIconModule } from '@angular/material/icon'
import type { ShortcutId } from '../../models/shortcuts'
import { IconButtonComponent } from '../button/icon-button.component'
import { KeyTagComponent } from '../shortcut/key-tag.component'
import { ShortcutDirective } from '../shortcut/shortcut.directive'

@Component({
  selector: 'app-search-bar',
  imports: [FormField, IconButtonComponent, KeyTagComponent, MatIconModule, ShortcutDirective],
  templateUrl: './search-bar.component.html',
  styleUrl: './search-bar.component.scss',
})
export class SearchBarComponent {
  readonly placeholder = input('Search…')
  /** The accessible name; the placeholder when unset. */
  readonly label = input<string>()
  readonly clearLabel = input('Clear search')
  /** The shortcut that focuses the field, of the view it is in; its key tag shows while empty. */
  readonly shortcut = input<ShortcutId | null>(null)
  readonly value = model('')
  protected readonly query = form(this.value)

  protected clear(): void {
    this.value.set('')
  }
}
