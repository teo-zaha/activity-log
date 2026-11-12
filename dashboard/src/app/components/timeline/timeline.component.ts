import { Component, computed, inject } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { ActivityService } from '../../services/activity.service'
import { ProviderIconComponent } from '../provider-icon/provider-icon.component'
import { SearchBarComponent } from '../search-bar/search-bar.component'
import type { EntryUiAdapter } from '../../providers/types'
import { localDateKey, relativeDayLabel } from '../../utils/date-labels'

interface TimelineEmptyState {
  readonly icon: string
  readonly title: string
  readonly message: string
  readonly action: 'clear-search' | null
}

@Component({
  selector: 'app-timeline',
  imports: [MatIconModule, ProviderIconComponent, SearchBarComponent],
  templateUrl: './timeline.component.html',
  styleUrl: './timeline.component.scss',
})
export class TimelineComponent {
  protected readonly activity = inject(ActivityService)

  /** Row widths (in %) for the loading skeleton. */
  protected readonly skeletonGroups = [
    [72, 54, 64],
    [58, 80],
  ]

  protected readonly summary = computed(() => {
    const groups = this.activity.groupedByDay()
    const events = groups.reduce((total, group) => total + group.entries.length, 0)
    const query = this.activity.searchQuery().trim()
    const base = `${events} ${events === 1 ? 'event' : 'events'} across ${groups.length} ${groups.length === 1 ? 'day' : 'days'}`
    return query ? `${base} matching “${query}”` : base
  })

  protected readonly emptyState = computed<TimelineEmptyState>(() => {
    const query = this.activity.searchQuery().trim()
    const status = this.activity.collectionStatus()
    if (status.kind === 'timedOut' || status.kind === 'failed') {
      return {
        icon: 'cloud_off',
        title: 'No activity loaded',
        message:
          'The last collection did not finish. Retry above, or check source status in Settings.',
        action: null,
      }
    }
    if (query) {
      return {
        icon: 'search_off',
        title: `No matches for “${query}”`,
        message: 'Try a different term, or search by repo, branch, ticket key or author.',
        action: 'clear-search',
      }
    }
    if (this.activity.providers().length === 0) {
      return {
        icon: 'power_off',
        title: 'No sources are ready',
        message: 'Open Settings to see why each source is unavailable and configure it.',
        action: null,
      }
    }
    if (this.activity.activeSources().size === 0) {
      return {
        icon: 'filter_alt_off',
        title: 'All sources are hidden',
        message: 'Turn on at least one source in the sidebar to see its activity.',
        action: null,
      }
    }
    return {
      icon: 'event_busy',
      title: 'Nothing recorded in this range',
      message: 'Pick a wider date range and collect again.',
      action: null,
    }
  })

  private readonly todayKey = localDateKey(new Date())

  protected selectEntry(entry: EntryUiAdapter): void {
    this.activity.selectedEntries.set(null)
    this.activity.selectedEntry.set(entry)
  }

  protected relativeDay(dateStr: string): string | null {
    return relativeDayLabel(dateStr, this.todayKey)
  }

  protected formatDate(dateStr: string): string {
    return new Date(dateStr + 'T12:00:00Z').toLocaleDateString('en-GB', {
      weekday: 'long',
      year: 'numeric',
      month: 'long',
      day: 'numeric',
    })
  }

  protected formatTime(ts: string): string {
    return new Date(ts).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
  }

  protected entryAriaLabel(entry: EntryUiAdapter): string {
    return `Open ${entry.badge.label} event: ${entry.title} at ${this.formatTime(entry.timestamp)}`
  }
}
