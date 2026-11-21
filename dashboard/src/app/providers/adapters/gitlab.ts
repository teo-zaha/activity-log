import type { GitLabEntry } from '../../generated/activity'
import { type ChangeReferencePrefix, changeReference, changeStackVmOf } from '../change-stack'
import { EntryUiAdapter, ProviderUiAdapterDefinition, type ProviderDetailField } from '../types'
import { createBadge, joinParts, toDefinedStrings } from '../utils'

/** How GitLab writes a merge request's number: `!12`. */
const MERGE_REQUEST: ChangeReferencePrefix = '!'

class GitLabEntryUiAdapter extends EntryUiAdapter<GitLabEntry> {
  constructor(provider: GitLabProviderAdapter, entry: GitLabEntry) {
    const fields: ProviderDetailField[] = [
      { label: 'Action', value: entry.type },
      { label: 'Title', value: entry.title ?? '' },
      { label: 'Target type', value: entry.targetType ?? '' },
    ]
    if (entry.mrIid != null)
      fields.push({ label: 'MR', value: changeReference(MERGE_REQUEST, entry.mrIid) })
    if (entry.issueIid != null) fields.push({ label: 'Issue', value: `#${entry.issueIid}` })
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

    super(provider, entry, {
      title: entry.title || entry.type,
      meta: joinParts([entry.type, entry.targetType]),
      entryIcon,
      searchTerms: toDefinedStrings([
        entry.title,
        entry.type,
        entry.targetType,
        entry.branch,
        entry.noteBody,
        entry.noteFile,
      ]),
      calendarDurationMinutes: null,
      changeStack: entry.changeStack ? changeStackVmOf(entry.changeStack, MERGE_REQUEST) : null,
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
