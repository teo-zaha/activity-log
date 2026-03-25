import { Component, inject } from '@angular/core'
import type { ActivityEntry, GitEntry, GitLabEntry } from '../../generated/activity'
import { ActivityService } from '../../services/activity.service'
import { isMergedEntry } from '../../utils/activity-entry'
import { formatDuration, getEntryIcon, getEntryTitle } from '../../utils/activity-presentation'

export interface DetailField {
  label: string
  value: string
}
export interface DetailSection {
  title: string
  fields: DetailField[]
}

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
  protected getEntryIcon = getEntryIcon
  protected getEntryTitle = getEntryTitle
  protected asMerged(e: ActivityEntry) {
    return e as GitEntry & { gitlabEvent: GitLabEntry }
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

  protected getFields(e: ActivityEntry): DetailField[] {
    switch (e.source) {
      case 'git':
        return [
          { label: 'Repository', value: e.repo },
          { label: 'Branch', value: e.branch },
          { label: 'Commit', value: e.hash },
          { label: 'Author', value: e.author },
        ].filter(f => f.value)
      case 'gitlab': {
        const fields: DetailField[] = [
          { label: 'Action', value: e.type },
          { label: 'Project', value: e.title },
        ]
        if (e.mr_iid != null) fields.push({ label: 'MR', value: `!${e.mr_iid}` })
        if (e.branch) fields.push({ label: 'Branch', value: e.branch })
        if (e.commit_count != null) fields.push({ label: 'Commits', value: String(e.commit_count) })
        if (e.note_file) fields.push({ label: 'File', value: e.note_file })
        if (e.note_body) fields.push({ label: 'Comment', value: e.note_body })
        if (e.note_resolved != null)
          fields.push({ label: 'Resolved', value: e.note_resolved ? 'Yes' : 'No' })
        return fields.filter(f => f.value)
      }
      case 'jira':
        return [
          { label: 'Key', value: e.key },
          { label: 'Status', value: e.status },
          { label: 'Issue type', value: e.issue_type },
          { label: 'From status', value: e.from_status ?? '' },
          { label: 'To status', value: e.to_status ?? '' },
        ].filter(f => f.value)
      case 'outlook':
        return [
          { label: 'Duration', value: formatDuration(e.duration_minutes) },
          { label: 'Location', value: e.location ?? '' },
          { label: 'Organizer', value: e.organizer ?? '' },
          { label: 'Role', value: e.is_organizer ? 'Organizer' : 'Attendee' },
        ].filter(f => f.value)
      case 'system':
        return [{ label: 'Event type', value: e.type }]
    }
  }

  protected getOverflowSections(
    entries: ActivityEntry[]
  ): (DetailSection & { icon: string; badgeClass: string; entryTitle: string })[] {
    return entries.map(e => ({
      title: e.source,
      icon: this.getEntryIcon(e),
      badgeClass: 'badge-' + e.source,
      entryTitle: this.getEntryTitle(e),
      fields: this.getFields(e),
    }))
  }

  protected getMergedSections(e: GitEntry & { gitlabEvent: GitLabEntry }): DetailSection[] {
    const gitFields: DetailField[] = [
      { label: 'Repository', value: e.repo },
      { label: 'Branch', value: e.branch },
      { label: 'Commit', value: e.hash },
      { label: 'Author', value: e.author },
    ].filter(f => f.value)

    const gl = e.gitlabEvent
    const gitlabFields: DetailField[] = [
      { label: 'Action', value: gl.type },
      { label: 'Project', value: gl.title },
    ]
    if (gl.branch) gitlabFields.push({ label: 'Branch', value: gl.branch })
    if (gl.commit_count != null)
      gitlabFields.push({ label: 'Commits', value: String(gl.commit_count) })
    if (gl.mr_iid != null) gitlabFields.push({ label: 'MR', value: `!${gl.mr_iid}` })

    return [
      { title: 'git', fields: gitFields.filter(f => f.value) },
      { title: 'gitlab', fields: gitlabFields.filter(f => f.value) },
    ]
  }
}
