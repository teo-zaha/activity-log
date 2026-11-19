import { Component, type ElementRef, afterNextRender, inject, viewChild } from '@angular/core'
import { windowFocus } from '@signality/core/browser/window-focus'
import { WindowChromeService } from '../../services/window-chrome.service'

/**
 * Minimize, maximize and close for the Windows window, which has no title bar: drawn like the
 * system's own, in its icon font, at the top right above everything else, and dimmed while the
 * window is in the background.
 */
@Component({
  selector: 'app-window-controls',
  templateUrl: './window-controls.component.html',
  styleUrl: './window-controls.component.scss',
  host: { '[class.is-inactive]': '!focused()' },
})
export class WindowControlsComponent {
  protected readonly chrome = inject(WindowChromeService)
  protected readonly focused = windowFocus()
  private readonly maximizeButton = viewChild.required<ElementRef<HTMLElement>>('maximize')

  constructor() {
    afterNextRender(
      () => void this.chrome.attachMaximizeButton(this.maximizeButton().nativeElement)
    )
  }
}
