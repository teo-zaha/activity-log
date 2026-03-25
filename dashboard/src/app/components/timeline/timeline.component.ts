import { Component, effect, inject } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { MatIconModule } from '@angular/material/icon'
import { ActivityService } from '../../services/activity.service'
import type { ActivityEntry } from '../../generated/activity'
import { isMergedEntry } from '../../utils/activity-entry'
import { getEntryIcon, getEntryMeta, getEntryTitle } from '../../utils/activity-presentation'

@Component({
  selector: 'app-timeline',
  standalone: true,
  imports: [FormsModule, MatIconModule],
  templateUrl: './timeline.component.html',
  styleUrl: './timeline.component.scss',
})
export class TimelineComponent {
  protected readonly activity = inject(ActivityService)
  protected searchValue = ''
  protected getEntryIcon = getEntryIcon
  protected getEntryMeta = getEntryMeta
  protected getEntryTitle = getEntryTitle

  constructor() {
    effect(() => { this.searchValue = this.activity.searchQuery() })
  }

  protected onSearch(value: string): void {
    this.activity.searchQuery.set(value)
  }

  protected selectEntry(e: ActivityEntry): void {
    this.activity.selectedEntry.set(e)
  }

  protected formatDate(dateStr: string): string {
    return new Date(dateStr + 'T12:00:00Z').toLocaleDateString('en-GB', {
      weekday: 'long', year: 'numeric', month: 'long', day: 'numeric',
    })
  }

  protected formatTime(ts: string): string {
    return new Date(ts).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
  }

  protected isMerged = isMergedEntry
}
