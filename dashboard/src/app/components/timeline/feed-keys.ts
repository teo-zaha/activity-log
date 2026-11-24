import type { ElementRef, Signal } from '@angular/core'
import { listener } from '@signality/core/browser/listener'

const STEPS: Readonly<Record<string, 1 | -1>> = { PageDown: 1, PageUp: -1 }

interface FeedKeysOptions {
  readonly scroller: Signal<ElementRef<HTMLElement>>
  readonly count: () => number
  readonly focusDay: (index: number) => void
  readonly tab: (target: HTMLElement, step: 1 | -1) => boolean
}

/** Page keys step through feed articles. Tab follows entry order even across virtual gaps. */
export function followFeedKeys({ scroller, count, focusDay, tab }: FeedKeysOptions): void {
  listener(scroller, 'keydown', (event: KeyboardEvent) => {
    const target = event.target
    if (!(target instanceof HTMLElement) || event.altKey || event.ctrlKey || event.metaKey) return
    if (event.key === 'Tab') {
      if (tab(target, event.shiftKey ? -1 : 1)) event.preventDefault()
      return
    }
    const step = STEPS[event.key]
    const article = target.closest<HTMLElement>('article[data-index]')
    if (step === undefined || event.shiftKey || !article) return
    const next = Number(article.dataset['index']) + step
    if (next < 0 || next >= count()) return
    event.preventDefault()
    focusDay(next)
  })
}
