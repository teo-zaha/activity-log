import { type ElementRef, Injector, type Signal, afterNextRender, inject } from '@angular/core'
import { listener } from '@signality/core/browser/listener'

/** The keys of the ARIA feed pattern: to the next or the previous article. */
const STEPS: Readonly<Record<string, 1 | -1>> = { PageDown: 1, PageUp: -1 }

export interface FeedKeysOptions {
  /** What holds the feed; its keys are followed here, the shortcut service's being the page's. */
  readonly scroller: Signal<ElementRef<HTMLElement>>
  /** How many articles the feed has, drawn or not. */
  readonly count: () => number
  /** Brings the article of `index` to the top of the screen, drawing it. */
  readonly scrollTo: (index: number) => void
}

/** The article of `index` among those drawn under `root`, by its `data-index`. */
function articleAt(root: HTMLElement, index: number): HTMLElement | null {
  return root.querySelector<HTMLElement>(`article[data-index="${index}"]`)
}

/**
 * Page Down and Page Up inside the feed move focus to the next or previous article, as the ARIA
 * feed pattern has it: an article not drawn yet is scrolled to first, then focused once drawn.
 * Unmodified keys only; the page's own Page Down and Page Up still scroll outside the feed.
 */
export function followFeedKeys({ scroller, count, scrollTo }: FeedKeysOptions): void {
  const injector = inject(Injector)
  listener(scroller, 'keydown', (event: KeyboardEvent) => {
    const step = STEPS[event.key]
    if (step === undefined || event.altKey || event.ctrlKey || event.metaKey || event.shiftKey) {
      return
    }
    const article =
      event.target instanceof Element ? event.target.closest<HTMLElement>('article') : null
    const index = Number(article?.dataset['index'])
    if (article === null || !Number.isInteger(index)) return
    const next = index + step
    if (next < 0 || next >= count()) return
    event.preventDefault()
    const root = scroller().nativeElement
    scrollTo(next)
    const drawn = articleAt(root, next)
    if (drawn !== null) drawn.focus({ preventScroll: true })
    else afterNextRender(() => articleAt(root, next)?.focus({ preventScroll: true }), { injector })
  })
}
