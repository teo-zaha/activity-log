import { NgTemplateOutlet } from '@angular/common'
import { Component, type TemplateRef, computed, inject, input } from '@angular/core'
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
  /**
   * Tauri's `data-tauri-drag-region` on what moves the window: the strip above the row, the row
   * and its empty middle themselves (`''`, not what lies in them) and the whole brand
   * (`'deep'`); the controls never do. Off in fullscreen and in a browser.
   */
  protected readonly dragsItself = computed(() => (this.chrome.movesWindow() ? '' : null))
  protected readonly dragsWithin = computed(() => (this.chrome.movesWindow() ? 'deep' : null))

  protected exportMd(): void {
    void this.exports.exportShown('markdown')
  }

  protected exportJson(): void {
    void this.exports.exportShown('json')
  }
}
