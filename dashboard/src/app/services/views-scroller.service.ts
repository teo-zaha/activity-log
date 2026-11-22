import { ViewportScroller } from '@angular/common'
import {
  DestroyRef,
  type ElementRef,
  Service,
  type Signal,
  afterNextRender,
  inject,
} from '@angular/core'

/**
 * What a view says of where it is beyond the pixels: a list whose content changes between leaving
 * and coming back (the Timeline, as a collection comes in) shows again the row it was at.
 */
export interface ScrollPlaces {
  /** Where the view is now, as a way back to it; null where the pixels say it all. */
  save(): (() => boolean) | null
}

/** A view whose scroller the router saves and restores, see `restoresScroll`. */
interface ScrolledView {
  readonly scroller: () => HTMLElement
  /** How far down a new navigation shows it: the top, or the calendar's default hour. */
  readonly start: () => number
  readonly places: ScrollPlaces | null
  /** Drawn at least once: its scroller is there and as tall as its content. */
  drawn: boolean
}

/** A way back to a place, and the view it belongs to. */
interface WayBack {
  readonly view: ScrolledView
  /** Shows the place; false where it is gone, and the pixels are shown instead. */
  readonly back: () => boolean
}

/**
 * The router's `ViewportScroller` (`withInMemoryScrolling`, `provideAppRouter`): the views scroll
 * inside the shell, each in a scroller of its own, not the window. The router saves the position
 * of the view on screen as a navigation starts and, on Back and Forward, asks for the one its
 * history entry was left at; a new navigation asks for `[0, 0]`.
 *
 * A position is measured from where the view starts (`ScrolledView.start`), so `[0, 0]` shows a
 * view where it starts: the Timeline at its top, a calendar at its default hour. A navigation
 * that keeps the view where it is says so (`scroll: 'manual'`, `ActivityViewStateService`).
 *
 * A view with places (`ScrollPlaces`) also saves where it is beyond the pixels, tied to the very
 * position the router keeps: the router hands that same array back on Back and Forward, and the
 * place is shown in place of its pixels while it is there. A position the router made anew (a new
 * navigation's `[0, 0]`) has none.
 *
 * The router asks once the navigation has ended, which may be before the view it put in is drawn
 * (a view's code loads, or its first render comes later): the position then waits for that draw.
 * Only the views' scrollers are kept: the details and the settings are layers over the view,
 * their navigations keep its position, and their own panes start at the top as they open.
 */
@Service()
export class ViewsScroller extends ViewportScroller {
  /** The views in the order they came; the Timeline stays while it is kept out of the document. */
  private readonly views: ScrolledView[] = []
  /** The position asked for while the view that came last was not drawn yet. */
  private waiting: [number, number] | null = null
  /** The places saved with the positions the router keeps, as long as it keeps them. */
  private readonly ways = new WeakMap<[number, number], WayBack>()

  /** Follows `view` until the returned function is called, as it is destroyed. */
  add(view: ScrolledView): () => void {
    this.views.push(view)
    return () => {
      const index = this.views.indexOf(view)
      if (index >= 0) this.views.splice(index, 1)
    }
  }

  /** `view` was first drawn: it shows where the router asked meanwhile, else where it starts. */
  drawn(view: ScrolledView): void {
    view.drawn = true
    const position = this.waiting ?? [0, 0]
    this.waiting = null
    if (this.shown() === view) this.show(view, position)
  }

  override getScrollPosition(): [number, number] {
    const view = this.shown()
    if (view === undefined) return [0, 0]
    const scroller = view.scroller()
    const position: [number, number] = [scroller.scrollLeft, scroller.scrollTop - view.start()]
    const back = view.places?.save() ?? null
    if (back !== null) this.ways.set(position, { view, back })
    return position
  }

  override scrollToPosition(position: [number, number]): void {
    if (this.views.at(-1)?.drawn === false) {
      this.waiting = position
      return
    }
    const view = this.shown()
    if (view !== undefined) this.show(view, position)
  }

  /** Shows `view` at the place saved with `position`, else at its pixels. */
  private show(view: ScrolledView, position: [number, number]): void {
    const way = this.ways.get(position)
    if (way?.view === view && way.back()) return
    const [left, top] = position
    const scroller = view.scroller()
    scroller.scrollLeft = left
    scroller.scrollTop = view.start() + top
  }

  /** The app's URLs have no fragments, so there is nothing to scroll to (`anchorScrolling` off). */
  override scrollToAnchor(): void {}

  /** Only `scrollToAnchor` would take an offset into account. */
  override setOffset(): void {}

  /**
   * The document itself never scrolls: the browser has nothing of its own to restore, and the
   * router restores the views' scrollers.
   */
  override setHistoryScrollRestoration(): void {}

  /** The view on screen: the latest one drawn that is in the document. */
  private shown(): ScrolledView | undefined {
    return [...this.views].reverse().find(view => view.drawn && view.scroller().isConnected)
  }
}

/**
 * Has the router save and restore where the view's `scroller` is scrolled to, per history entry
 * (`ViewsScroller`). Shown first at `start` pixels down, as a new navigation shows it again; with
 * `places`, Back and Forward show the place saved, where it is still there.
 */
export function restoresScroll(
  scroller: Signal<ElementRef<HTMLElement>>,
  start: () => number = () => 0,
  places: ScrollPlaces | null = null
): void {
  const views = inject(ViewsScroller)
  const view: ScrolledView = {
    scroller: () => scroller().nativeElement,
    start,
    places,
    drawn: false,
  }
  inject(DestroyRef).onDestroy(views.add(view))
  afterNextRender({ write: () => views.drawn(view) })
}
