import { Component, ElementRef, input, model, viewChild } from '@angular/core'
import { form, FormField } from '@angular/forms/signals'
import { MatIconModule } from '@angular/material/icon'
import { onKey } from '@signality/core/elements/on-key'
import { IconButtonComponent } from '../button/icon-button.component'

@Component({
  selector: 'app-search-bar',
  imports: [IconButtonComponent, MatIconModule, FormField],
  templateUrl: './search-bar.component.html',
  styleUrl: './search-bar.component.scss',
})
export class SearchBarComponent {
  readonly placeholder = input('Search…')
  /** The accessible name; the placeholder when unset. */
  readonly label = input<string>()
  readonly clearLabel = input('Clear search')
  readonly value = model('')
  protected readonly query = form(this.value)

  private readonly input = viewChild.required<ElementRef<HTMLInputElement>>('input')

  constructor() {
    onKey('/', event => this.focusOnSlash(event), { target: document })
  }

  protected clear(): void {
    this.value.set('')
  }

  /**
   * "/" focuses the field unless the user is already typing somewhere, or the field is out of the
   * document (the timeline's, kept by the router while a calendar view shows).
   */
  private focusOnSlash(event: KeyboardEvent): void {
    if (event.metaKey || event.ctrlKey || event.altKey) return
    if (!this.input().nativeElement.isConnected) return
    const target = event.target
    if (
      target instanceof HTMLElement &&
      (target.isContentEditable || target.closest('input, textarea, select, [role="dialog"]'))
    ) {
      return
    }
    event.preventDefault()
    this.input().nativeElement.focus()
  }
}
