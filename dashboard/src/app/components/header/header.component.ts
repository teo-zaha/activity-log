import { NgTemplateOutlet } from '@angular/common'
import { Component, type TemplateRef, inject, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { MatTooltipModule } from '@angular/material/tooltip'
import { IconButtonComponent } from '../button/icon-button.component'
import { ActivityExportService } from '../../services/activity-export.service'
import { WindowChromeService } from '../../services/window-chrome.service'

/**
 * The window's top row on every page: the brand, what the layout on screen puts in the middle
 * (`center`), and Export. In the desktop app it is the title bar too.
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

  private readonly exports = inject(ActivityExportService)
  protected readonly canExport = this.exports.canExport
  private readonly chrome = inject(WindowChromeService)

  protected exportMd(): void {
    void this.exports.exportShown('markdown')
  }

  protected exportJson(): void {
    void this.exports.exportShown('json')
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
