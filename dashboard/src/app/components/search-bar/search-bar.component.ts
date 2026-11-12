import { Component, ElementRef, input, model, viewChild } from '@angular/core'
import { form, FormField } from '@angular/forms/signals'

@Component({
  selector: 'app-search-bar',
  imports: [FormField],
  templateUrl: './search-bar.component.html',
  styleUrl: './search-bar.component.scss',
  host: {
    '(document:keydown)': 'focusOnSlash($event)',
  },
})
export class SearchBarComponent {
  readonly placeholder = input('Search…')
  readonly value = model('')
  protected readonly query = form(this.value)

  private readonly input = viewChild.required<ElementRef<HTMLInputElement>>('input')

  protected clear(): void {
    this.value.set('')
  }

  /** "/" focuses search unless the user is already typing somewhere. */
  protected focusOnSlash(event: KeyboardEvent): void {
    if (event.key !== '/' || event.metaKey || event.ctrlKey || event.altKey) return
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
