import { Component, inject } from '@angular/core'
import { ActivityService } from '../../services/activity.service'
import type { ActivityEntry, GitEntry, GitLabEntry, JiraEntry, OutlookEntry, SystemEntry } from '../../models/activity'
import { isMergedEntry } from '../../models/activity'

export interface DetailField   { label: string; value: string }
export interface DetailSection { title: string; fields: DetailField[] }

@Component({
  selector: 'app-event-detail',
  standalone: true,
  imports: [],
  templateUrl: './event-detail.component.html',
  styleUrl: './event-detail.component.scss',
})
export class EventDetailComponent {
  protected readonly activity = inject(ActivityService)

  protected isMerged = isMergedEntry
  protected asMerged(e: ActivityEntry) { return e as GitEntry & { gitlabEvent: GitLabEntry } }

  protected close(): void {
    this.activity.selectedEntry.set(null)
    this.activity.selectedEntries.set(null)
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
        const fields: DetailField[] = [
          { label: 'Action',  value: g.type },
          { label: 'Project', value: g.title },
        ]
        if (g.mr_iid != null)       fields.push({ label: 'MR',           value: `!${g.mr_iid}` })
        if (g.branch)               fields.push({ label: 'Branch',        value: g.branch })
        if (g.commit_count != null) fields.push({ label: 'Commits',       value: String(g.commit_count) })
        if (g.note_file)            fields.push({ label: 'File',          value: g.note_file })
        if (g.note_body)            fields.push({ label: 'Comment',       value: g.note_body })
        if (g.note_resolved != null) fields.push({ label: 'Resolved',     value: g.note_resolved ? 'Yes' : 'No' })
        return fields.filter(f => f.value)
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

  protected getOverflowSections(entries: ActivityEntry[]): (DetailSection & { icon: string; badgeClass: string; entryTitle: string })[] {
    return entries.map(e => ({
      title: e.source,
      icon: this.getSourceIcon(e.source, e.type),
      badgeClass: 'badge-' + e.source,
      entryTitle: this.getTitle(e),
      fields: this.getFields(e),
    }))
  }

  protected getMergedSections(e: GitEntry & { gitlabEvent: GitLabEntry }): DetailSection[] {
    const gitFields: DetailField[] = [
      { label: 'Repository', value: e.repo },
      { label: 'Branch',     value: e.branch },
      { label: 'Commit',     value: e.hash },
      { label: 'Author',     value: e.author },
    ].filter(f => f.value)

    const gl = e.gitlabEvent
    const gitlabFields: DetailField[] = [
      { label: 'Action',  value: gl.type },
      { label: 'Project', value: gl.title },
    ]
    if (gl.branch)               gitlabFields.push({ label: 'Branch',   value: gl.branch })
    if (gl.commit_count != null) gitlabFields.push({ label: 'Commits',  value: String(gl.commit_count) })
    if (gl.mr_iid != null)       gitlabFields.push({ label: 'MR',       value: `!${gl.mr_iid}` })

    return [
      { title: 'git',    fields: gitFields.filter(f => f.value) },
      { title: 'gitlab', fields: gitlabFields.filter(f => f.value) },
    ]
  }
}
