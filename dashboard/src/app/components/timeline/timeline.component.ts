import { Component, inject } from '@angular/core'
import { FormsModule } from '@angular/forms'
import { MatIconModule } from '@angular/material/icon'
import { ActivityService } from '../../services/activity.service'
import type { ActivityEntry, GitEntry, GitLabEntry, JiraEntry, OutlookEntry, SystemEntry } from '../../models/activity'

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

  protected onSearch(value: string): void {
    this.activity.searchQuery.set(value)
  }

  protected selectEntry(e: ActivityEntry): void {
    this.activity.selectedEntry.set(e)
  }

  protected getSourceIcon(source: string, type?: string): string {
    if (source === 'system') {
      const t: Record<string, string> = {
        boot: 'power_settings_new', shutdown: 'power_off',
        unexpected_shutdown: 'warning', sleep: 'bedtime',
        wake: 'wb_sunny', lock: 'lock', unlock: 'lock_open',
      }
      return t[type ?? ''] ?? 'power_settings_new'
    }
    const icons: Record<string, string> = {
      git: 'commit', gitlab: 'code', jira: 'task_alt', outlook: 'event',
    }
    return icons[source] ?? 'circle'
  }

  protected formatDate(dateStr: string): string {
    return new Date(dateStr + 'T12:00:00Z').toLocaleDateString('en-GB', {
      weekday: 'long', year: 'numeric', month: 'long', day: 'numeric',
    })
  }

  protected formatTime(ts: string): string {
    return new Date(ts).toLocaleTimeString('en-GB', { hour: '2-digit', minute: '2-digit' })
  }

  protected getTitle(e: ActivityEntry): string {
    switch (e.source) {
      case 'git':     return (e as GitEntry).message ?? ''
      case 'gitlab':  return (e as GitLabEntry).title || (e as GitLabEntry).type
      case 'jira':  { const j = e as JiraEntry; return j.key ? `${j.key}: ${j.title ?? ''}` : (j.title ?? '') }
      case 'outlook': return (e as OutlookEntry).title ?? ''
      case 'system':  return (e as SystemEntry).label ?? e.type
    }
  }

  protected getBadgeLabel(e: ActivityEntry): string {
    return e.source
  }

  protected getMeta(e: ActivityEntry): string {
    switch (e.source) {
      case 'git': {
        const g = e as GitEntry
        return [g.repo, g.branch ? `${g.branch}` : '', g.hash?.slice(0, 7)].filter(Boolean).join(' · ')
      }
      case 'gitlab': {
        const g = e as GitLabEntry
        return [g.type, g.target_type].filter(Boolean).join(' · ')
      }
      case 'jira': {
        const j = e as JiraEntry
        return [j.issue_type, j.status].filter(Boolean).join(' · ')
      }
      case 'outlook': {
        const o = e as OutlookEntry
        const dur = o.duration_minutes >= 60
          ? `${Math.floor(o.duration_minutes / 60)}h${o.duration_minutes % 60 ? ` ${o.duration_minutes % 60}m` : ''}`
          : `${o.duration_minutes}m`
        return [
          dur,
          o.location,
          o.is_organizer ? 'organizer' : 'attendee',
          o.organizer && !o.is_organizer ? `org: ${o.organizer}` : '',
        ].filter(Boolean).join(' · ')
      }
      case 'system':
        return (e as SystemEntry).type
    }
  }
}
