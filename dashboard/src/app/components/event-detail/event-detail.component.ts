import { Component, inject } from '@angular/core'
import { ActivityService } from '../../services/activity.service'
import type { ActivityEntry, GitEntry, GitLabEntry, JiraEntry, OutlookEntry, SystemEntry } from '../../models/activity'

export interface DetailField { label: string; value: string }

@Component({
  selector: 'app-event-detail',
  standalone: true,
  imports: [],
  templateUrl: './event-detail.component.html',
  styleUrl: './event-detail.component.scss',
})
export class EventDetailComponent {
  protected readonly activity = inject(ActivityService)

  protected close(): void {
    this.activity.selectedEntry.set(null)
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

  protected formatDateTime(ts: string): string {
    return new Date(ts).toLocaleString('en-GB', {
      weekday: 'long', year: 'numeric', month: 'long',
      day: 'numeric', hour: '2-digit', minute: '2-digit',
    })
  }

  protected getTitle(e: ActivityEntry): string {
    switch (e.source) {
      case 'git':     return (e as GitEntry).message ?? ''
      case 'gitlab':  return (e as GitLabEntry).title || (e as GitLabEntry).type
      case 'jira':    return (e as JiraEntry).title ?? ''
      case 'outlook': return (e as OutlookEntry).title ?? ''
      case 'system':  return (e as SystemEntry).label ?? e.type
    }
  }

  protected getFields(e: ActivityEntry): DetailField[] {
    switch (e.source) {
      case 'git': {
        const g = e as GitEntry
        return [
          { label: 'Repository', value: g.repo },
          { label: 'Branch',     value: g.branch },
          { label: 'Commit',     value: g.hash },
          { label: 'Author',     value: g.author },
        ].filter(f => f.value)
      }
      case 'gitlab': {
        const g = e as GitLabEntry
        return [
          { label: 'Action',      value: g.type },
          { label: 'Target type', value: g.target_type },
        ].filter(f => f.value)
      }
      case 'jira': {
        const j = e as JiraEntry
        return [
          { label: 'Key',        value: j.key },
          { label: 'Status',     value: j.status },
          { label: 'Issue type', value: j.issue_type },
        ].filter(f => f.value)
      }
      case 'outlook': {
        const o = e as OutlookEntry
        const dur = o.duration_minutes >= 60
          ? `${Math.floor(o.duration_minutes / 60)}h${o.duration_minutes % 60 ? ` ${o.duration_minutes % 60}m` : ''}`
          : `${o.duration_minutes}m`
        return [
          { label: 'Duration',   value: dur },
          { label: 'Location',   value: o.location ?? '' },
          { label: 'Organizer',  value: o.organizer ?? '' },
          { label: 'Role',       value: o.is_organizer ? 'Organizer' : 'Attendee' },
        ].filter(f => f.value)
      }
      case 'system': {
        const s = e as SystemEntry
        return [{ label: 'Event type', value: s.type }]
      }
    }
  }
}
