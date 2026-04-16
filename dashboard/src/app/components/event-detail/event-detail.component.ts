import { Component, inject } from '@angular/core'
import { ActivityService } from '../../services/activity.service'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'

@Component({
  selector: 'app-event-detail',
  standalone: true,
  imports: [ProviderIconComponent],
  templateUrl: './event-detail.component.html',
  styleUrl: './event-detail.component.scss',
})
export class EventDetailComponent {
  protected readonly activity = inject(ActivityService)

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
}
