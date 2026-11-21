import { NgTemplateOutlet } from '@angular/common'
import { Component, type TemplateRef, inject, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { IconButtonComponent } from '../button/icon-button.component'
import { ActivityService } from '../../services/activity.service'
import { WindowChromeService } from '../../services/window-chrome.service'
import { injectShareLink, injectViewLink } from '../share/share-link'

/**
 * The window's top row on every page: the brand, what the layout on screen puts in the middle
 * (`center`), and Export and Share. In the desktop app it is the title bar too.
 */
@Component({
  selector: 'app-header',
  imports: [IconButtonComponent, MatIconModule, MatTooltipModule, NgTemplateOutlet],
  templateUrl: './header.component.html',
  styleUrl: './header.component.scss',
})
export class HeaderComponent {
  /** The layout's part of the row, such as the views' range and Collect; none on other pages. */
  readonly center = input<TemplateRef<unknown> | null>(null)

  protected readonly activity = inject(ActivityService)
  private readonly chrome = inject(WindowChromeService)
  private readonly shareLink = injectShareLink()
  private readonly viewLink = injectViewLink()

  protected exportMd(): void {
    void this.activity.exportMarkdown()
  }

  protected exportJson(): void {
    void this.activity.exportJson()
  }

  /** Shares the link to this view (`injectViewLink`). */
  protected shareView(): Promise<void> {
    return this.shareLink(this.viewLink())
  }

  protected startHeaderDrag(event: MouseEvent): void {
    if (this.isDragSurface(event)) void this.startWindowDrag(event)
  }

  protected toggleHeaderMaximize(event: MouseEvent): void {
    if (this.isDragSurface(event)) void this.toggleWindowMaximize(event)
  }

  /** The row itself, its empty middle and the brand move the window; controls do not. */
  private isDragSurface(event: MouseEvent): boolean {
    const target = event.target
    return (
      target === event.currentTarget ||
      (target instanceof Element &&
        (target.classList.contains('header-center') || target.closest('.brand') !== null))
    )
  }

  protected startWindowDrag(event: MouseEvent): Promise<void> {
    return this.chrome.startDrag(event)
  }

  protected toggleWindowMaximize(event: MouseEvent): Promise<void> {
    return this.chrome.toggleMaximize(event)
  }
}
