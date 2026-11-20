import { Service } from '@angular/core'
import type { ConnectionKey } from '../../../providers/connection'
import { type SourceDraft, sameDraft } from './source-draft-request'

/**
 * Unsaved source edits by connection, kept while the app runs: closing the settings, going back
 * or following a link leaves them, and opening the connection again shows them, still unsaved,
 * until they are saved or discarded. Not a signal: the pane reads a draft once, when it loads.
 * Each stored draft has a revision, so a save clears the draft it sent and never a later one.
 */
@Service()
export class SourceDrafts {
  private readonly drafts = new Map<ConnectionKey, { draft: SourceDraft; revision: number }>()
  private revision = 0

  get(key: ConnectionKey): SourceDraft | undefined {
    return this.drafts.get(key)?.draft
  }

  /**
   * Stores `draft`; the revision names this draft of `key`. The draft stored already, shown
   * again, keeps its revision: only an edit makes a new one.
   */
  set(key: ConnectionKey, draft: SourceDraft): number {
    const stored = this.drafts.get(key)
    if (stored !== undefined && sameDraft(stored.draft, draft)) return stored.revision
    const revision = ++this.revision
    this.drafts.set(key, { draft, revision })
    return revision
  }

  delete(key: ConnectionKey): void {
    this.drafts.delete(key)
  }

  /** The revision of the draft of `key`, if any. */
  revisionOf(key: ConnectionKey): number | undefined {
    return this.drafts.get(key)?.revision
  }

  /**
   * Drops the draft of `key` that was saved or removed, unless it has been edited since
   * (`revision` undefined: there was none, so one made since stays).
   */
  settled(key: ConnectionKey, revision: number | undefined): void {
    if (this.drafts.get(key)?.revision === revision) this.drafts.delete(key)
  }
}
