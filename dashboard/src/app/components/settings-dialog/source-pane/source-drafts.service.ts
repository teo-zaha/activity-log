import { Service } from '@angular/core'
import type { ConnectionKey } from '../../../providers/connection'

/** A connection's unsaved edits, as the source pane holds them. */
export interface SourceDraft {
  readonly label: string
  /** Field values by key. A secret is `null` once the user chose to clear it. */
  readonly values: Readonly<Record<string, string | null>>
  /** Items of `list` fields by key. */
  readonly lists: Readonly<Record<string, readonly string[]>>
  /** List keys reset to their default. */
  readonly resets: readonly string[]
}

/**
 * Unsaved source edits by connection, kept while the app runs: closing the settings, going back
 * or following a link leaves them, and opening the connection again shows them, still unsaved,
 * until they are saved or discarded. Not a signal: the pane reads a draft once, when it loads.
 */
@Service()
export class SourceDrafts {
  private readonly drafts = new Map<ConnectionKey, SourceDraft>()

  get(key: ConnectionKey): SourceDraft | undefined {
    return this.drafts.get(key)
  }

  set(key: ConnectionKey, draft: SourceDraft): void {
    this.drafts.set(key, draft)
  }

  delete(key: ConnectionKey): void {
    this.drafts.delete(key)
  }
}
