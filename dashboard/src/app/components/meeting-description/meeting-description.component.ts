import {
  Component,
  ElementRef,
  afterRenderEffect,
  input,
  linkedSignal,
  signal,
  viewChild,
} from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { resizeObserver } from '@signality/core/observers/resize-observer'
import type { DescriptionBlock } from '../../generated/activity'
import { DescriptionRunsComponent } from './description-runs.component'

let nextId = 0

/**
 * A meeting description in the paragraphs, headings, lists and images the backend read it in,
 * its links clickable, cut to a few lines with a toggle when it is longer. The text is never
 * read as markup. An image that does not load, or whose data the feed lacks, shows as its alt
 * text; a decorative one (empty alt text) then shows nothing.
 */
@Component({
  selector: 'app-meeting-description',
  imports: [DescriptionRunsComponent, MatIconModule],
  templateUrl: './meeting-description.component.html',
  styleUrl: './meeting-description.component.scss',
})
export class MeetingDescriptionComponent {
  /** The description as the backend laid it out. */
  readonly blocks = input.required<readonly DescriptionBlock[]>()

  protected readonly id = `meeting-description-${nextId++}`
  /** Collapsed again whenever another description is shown. */
  protected readonly expanded = linkedSignal({ source: this.blocks, computation: () => false })
  /** Whether the clamped text hides lines, measured after rendering. */
  protected readonly overflowing = signal(false)
  /** The indexes of the image blocks that failed to load. */
  protected readonly failedImages = linkedSignal({
    source: this.blocks,
    computation: (): ReadonlySet<number> => new Set(),
  })
  private readonly body = viewChild.required<ElementRef<HTMLElement>>('body')

  constructor() {
    afterRenderEffect({
      read: () => {
        this.blocks()
        this.failedImages()
        this.measure()
      },
    })
    // A wider or narrower column scales the images and rewraps the text.
    resizeObserver(this.body, () => this.measure())
  }

  /** Measures again; also when an image is in, which changes the height of the text. */
  protected measure(): void {
    if (this.expanded()) return
    const element = this.body().nativeElement
    this.overflowing.set(element.scrollHeight > element.clientHeight + 1)
  }

  protected imageFailed(index: number): void {
    this.failedImages.update(failed => new Set(failed).add(index))
  }

  protected toggle(): void {
    this.expanded.update(expanded => !expanded)
  }
}
