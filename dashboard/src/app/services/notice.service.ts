import { Service, signal } from '@angular/core'
import type { Notice } from './failure'

/**
 * The snackbar's notice: a failure of an action that has no place of its own to show it, by the
 * action it came from. `AppComponent` shows the current one; nothing reads it to decide what the
 * sources or the collection are.
 */
@Service()
export class NoticeService {
  /** The notice on screen, null for none. `ActivityService.notice` is this same signal. */
  readonly current = signal<Notice | null>(null)

  show(notice: Notice): void {
    this.current.set(notice)
  }

  /** Clears the notice if it came from `origin` (a new attempt of the same action). */
  clear(origin: Notice['origin']): void {
    if (this.current()?.origin === origin) this.current.set(null)
  }
}
