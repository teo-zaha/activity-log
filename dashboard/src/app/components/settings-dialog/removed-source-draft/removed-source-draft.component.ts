import { Component, inject, input, output, signal } from '@angular/core'
import type { ConnectionId } from '../../../generated/activity'
import { type ConnectionKey, connectionKey } from '../../../providers/connection'
import { ActivityService } from '../../../services/activity.service'
import { type Failure, failure, failureOf } from '../../../services/failure'
import { ButtonComponent } from '../../button/button.component'
import { FeedbackComponent } from '../../feedback/feedback.component'
import { SourceDrafts } from '../source-pane/source-drafts.service'

/**
 * Unsaved changes of a source another program removed: kept until the user discards them or adds
 * the source again. Shown by the source pane once its recovery finds the source gone, and by the
 * settings dialog once the sources no longer list it.
 */
@Component({
  selector: 'app-removed-source-draft',
  imports: [ButtonComponent, FeedbackComponent],
  templateUrl: './removed-source-draft.component.html',
})
export class RemovedSourceDraftComponent {
  private readonly activity = inject(ActivityService)
  private readonly drafts = inject(SourceDrafts)

  /** The removed connection the draft was made for. */
  readonly connection = input.required<ConnectionId>()
  /** Its label, which the source added again gets. */
  readonly label = input.required<string>()
  /** The source added again, with the draft moved to it, to open. */
  readonly opened = output<ConnectionKey>()
  /** The draft was discarded. */
  readonly discarded = output<void>()

  protected readonly adding = signal(false)
  protected readonly failure = signal<Failure | null>(null)

  protected discard(): void {
    this.drafts.delete(connectionKey(this.connection()))
    this.discarded.emit()
  }

  /**
   * Adds the source again and moves the draft to the connection the backend created, which may
   * have another name than the removed one (another program may have restored that name): the
   * draft is never saved onto a connection the user did not add. It is based on the snapshot
   * that holds the new connection, so its save replaces only the fields it changes.
   */
  protected async addAgain(): Promise<void> {
    const removed = connectionKey(this.connection())
    const draft = this.drafts.get(removed)
    if (draft === undefined || this.adding()) return
    this.adding.set(true)
    this.failure.set(null)
    try {
      const answer = await this.activity.addConnection({
        source: this.connection().source,
        label: this.label(),
      })
      const standing = this.activity.receiptStanding(answer.installed)
      if (standing !== 'current' && standing !== 'superseded') {
        this.failure.set(
          failure('The source was added, but the app on screen does not show it yet; try again.')
        )
        return
      }
      const created = connectionKey(answer.created)
      this.drafts.delete(removed)
      this.drafts.set(created, { ...draft, basedOn: answer.snapshot.revision })
      this.opened.emit(created)
    } catch (error) {
      this.failure.set(failureOf(error, 'Failed to add the source again.'))
    } finally {
      this.adding.set(false)
    }
  }
}
