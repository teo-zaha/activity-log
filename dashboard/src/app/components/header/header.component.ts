import { NgTemplateOutlet } from '@angular/common'
import { Component, type TemplateRef, inject, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { Router } from '@angular/router'
import { IconButtonComponent } from '../button/icon-button.component'
import { ActivityService } from '../../services/activity.service'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'
import { WindowChromeService } from '../../services/window-chrome.service'
import { appLink, injectLinkBase, injectShareLink, pageLink } from '../share/share-link'

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
  private readonly router = inject(Router)
  private readonly shareLink = injectShareLink()
  private readonly linkBase = injectLinkBase()
  private readonly desktop = inject(RuntimeEnvironmentService).isTauri()

  protected exportMd(): void {
    void this.activity.exportMarkdown()
  }

  protected exportJson(): void {
    void this.activity.exportJson()
  }

  /**
   * Shares the link to this view, with the details open in it, if any. In the browser that is the
   * page's own address, its whole query included; the desktop app's link keeps what the view
   * shows.
   */
  protected shareView(): Promise<void> {
    return this.shareLink(
      this.desktop
        ? appLink(this.router, ['q', 'from', 'to', 'event', 'events'])
        : pageLink(this.router, this.linkBase)
    )
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
