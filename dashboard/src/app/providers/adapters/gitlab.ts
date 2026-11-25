import type { GitLabEntry } from '../../generated/activity'
import { changeReference, changeStackVmOf } from '../change-stack'
import { CODE_HOST_REFERENCE_PREFIX, gitlabAction, gitlabTargetLabel } from '../code-host'
import { EntryUiAdapter, ProviderUiAdapterDefinition, type ProviderDetailField } from '../types'
import { createBadge, joinParts, toDefinedStrings } from '../utils'

class GitLabEntryUiAdapter extends EntryUiAdapter<GitLabEntry> {
  constructor(provider: GitLabProviderAdapter, entry: GitLabEntry) {
    const action = gitlabAction(entry)
    const target = entry.targetType ? gitlabTargetLabel(entry.targetType) : undefined
    const fields: ProviderDetailField[] = [
      { label: 'Action', value: action.detail },
      { label: 'Title', value: entry.title ?? '' },
    ]
    if (entry.project) fields.push({ label: 'Project', value: entry.project })
    if (target) fields.push({ label: 'Target type', value: target })
    if (entry.mrIid != null)
      fields.push({
        label: 'MR',
        value: changeReference(CODE_HOST_REFERENCE_PREFIX.mergeRequest, entry.mrIid),
      })
    if (entry.issueIid != null)
      fields.push({
        label: 'Issue',
        value: changeReference(CODE_HOST_REFERENCE_PREFIX.issue, entry.issueIid),
      })
    if (entry.branch) fields.push({ label: 'Branch', value: entry.branch })
    if (entry.commitCount != null) {
      fields.push({ label: 'Commits', value: String(entry.commitCount) })
    }
    if (entry.noteFile) fields.push({ label: 'File', value: entry.noteFile })
    if (entry.noteBody) fields.push({ label: 'Comment', value: entry.noteBody })
    if (entry.noteResolved != null) {
      fields.push({ label: 'Resolved', value: entry.noteResolved ? 'Yes' : 'No' })
    }

    const entryIcon = 'code'
    const badge = createBadge(provider.source, provider.label, entryIcon)
    const webUrl = entry.webUrl ?? entry.changeStack?.current.webUrl
    const linkLabel = entry.issueIid != null ? 'Open issue' : 'Open merge request'

    super(provider, entry, {
      title: entry.title || action.summary,
      meta: joinParts([entry.project, entry.title ? action.summary : null]),
      entryIcon,
      searchTerms: toDefinedStrings([
        entry.title,
        entry.type,
        entry.targetType,
        entry.project,
        action.summary,
        target,
        entry.branch,
        entry.noteBody,
        entry.noteFile,
      ]),
      calendarDurationMinutes: null,
      changeStack: entry.changeStack
        ? changeStackVmOf(entry.changeStack, CODE_HOST_REFERENCE_PREFIX.mergeRequest)
        : null,
      detailLink: webUrl ? { label: linkLabel, url: webUrl } : null,
      detailFields: fields.filter(field => field.value),
      badge,
      badges: [badge],
      detailSections: [],
      styleKey: provider.source,
    })
  }
}

class GitLabProviderAdapter extends ProviderUiAdapterDefinition<
  'gitlab',
  GitLabEntry,
  GitLabEntryUiAdapter
> {
  readonly source = 'gitlab' as const
  readonly label = 'GitLab'
  readonly providerIcon = 'code'

  create(entry: GitLabEntry): GitLabEntryUiAdapter {
    return new GitLabEntryUiAdapter(this, entry)
  }
}

export const gitlabAdapter = new GitLabProviderAdapter()
