import { Component, ElementRef, input, model, viewChildren } from '@angular/core'

export interface SegmentedOption<T extends string> {
  readonly value: T
  readonly label: string
  /** A small count after the label. */
  readonly count?: number
  /** Tints the count. */
  readonly tone?: 'warn' | 'error'
}

/**
 * The option a radio group's key moves to from `current` among `count`: the arrows step and wrap,
 * Home and End go to the ends. Null for any other key.
 */
export function radioKeyTarget(key: string, current: number, count: number): number | null {
  const last = count - 1
  switch (key) {
    case 'ArrowRight':
    case 'ArrowDown':
      return current >= last ? 0 : current + 1
    case 'ArrowLeft':
    case 'ArrowUp':
      return current <= 0 ? last : current - 1
    case 'Home':
      return 0
    case 'End':
      return last
    default:
      return null
  }
}

/**
 * One choice out of a few, as a row of segments with radio-group semantics: only the chosen
 * segment is in the tab order, and the arrow keys move the choice.
 */
@Component({
  selector: 'app-segmented-control',
  templateUrl: './segmented-control.component.html',
  styleUrl: './segmented-control.component.scss',
  host: {
    role: 'radiogroup',
    '[attr.aria-label]': 'label()',
    '(keydown)': 'onKeydown($event)',
  },
})
export class SegmentedControlComponent<T extends string> {
  readonly options = input.required<readonly SegmentedOption<T>[]>()
  readonly value = model.required<T>()
  readonly label = input.required<string>()

  private readonly buttons = viewChildren<ElementRef<HTMLButtonElement>>('segment')

  protected onKeydown(event: KeyboardEvent): void {
    const options = this.options()
    const current = options.findIndex(option => option.value === this.value())
    const next = radioKeyTarget(event.key, current, options.length)
    if (next === null || !options[next]) return
    event.preventDefault()
    this.value.set(options[next].value)
    this.buttons()[next]?.nativeElement.focus()
  }
}
