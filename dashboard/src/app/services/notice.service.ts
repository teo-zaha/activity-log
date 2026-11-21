import { Service, signal, type Signal } from '@angular/core'
import type { Notice } from './failure'

/**
 * The snackbar's notice: a failure of an action that has no place of its own to show it, by the
 * action it came from. `AppComponent` shows the current one; nothing reads it to decide what the
 * sources or the collection are.
 */
@Service()
export class NoticeService {
  private readonly notice = signal<Notice | null>(null)

  /** The notice on screen, null for none. */
  readonly current: Signal<Notice | null> = this.notice.asReadonly()

  show(notice: Notice): void {
    this.notice.set(notice)
  }

  /** Clears the notice if it came from `origin` (a new attempt of the same action). */
  clear(origin: Notice['origin']): void {
    if (this.notice()?.origin === origin) this.notice.set(null)
  }
}
