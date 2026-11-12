import { A11yModule } from '@angular/cdk/a11y'
import { Component, effect, inject } from '@angular/core'
import { ActivityService } from '../../services/activity.service'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'

@Component({
  selector: 'app-event-detail',
  imports: [A11yModule, ProviderIconComponent],
  templateUrl: './event-detail.component.html',
  styleUrl: './event-detail.component.scss',
})
export class EventDetailComponent {
  protected readonly activity = inject(ActivityService)
  private previouslyFocusedElement: HTMLElement | null = null
  private wasOpen = false

  constructor() {
    effect(() => {
      const isOpen = this.isOpen()

      if (isOpen && !this.wasOpen) {
        const activeElement = document.activeElement
        this.previouslyFocusedElement = activeElement instanceof HTMLElement ? activeElement : null
      }

      if (!isOpen && this.wasOpen) {
        this.restoreFocus()
      }

      this.wasOpen = isOpen
    })
  }

  protected close(): void {
    this.activity.selectedEntry.set(null)
    this.activity.selectedEntries.set(null)
  }

  protected formatDateTime(ts: string): string {
    return new Date(ts).toLocaleString('en-GB', {
      weekday: 'long',
      year: 'numeric',
      month: 'long',
      day: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    })
  }

  protected isOpen(): boolean {
    return this.activity.selectedEntry() !== null || this.activity.selectedEntries() !== null
  }

  private restoreFocus(): void {
    const element = this.previouslyFocusedElement
    this.previouslyFocusedElement = null

    if (!element?.isConnected) {
      return
    }

    queueMicrotask(() => {
      element.focus()
    })
  }
}
