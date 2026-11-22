import {
  type ElementRef,
  Injector,
  type Signal,
  afterNextRender,
  afterRenderEffect,
  computed,
  inject,
  untracked,
} from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import { type EntryKey, parseEntryKey } from '../../providers/connection'
import { type DayKey, isDayKey } from '../../utils/local-day'
import type { ScrollPlaces } from '../../services/views-scroller.service'
import type { TimelineDay } from './timeline-rows'

/** What the reader is at: an entry's row, or a day where no row is under the line. */
type Mark =
  | { readonly kind: 'entry'; readonly key: EntryKey }
  | { readonly kind: 'day'; readonly day: DayKey }

/** The mark kept in place, and where it was when last read or placed. */
interface Anchor {
  readonly mark: Mark
  /** Its top in the scroller's content, which no scroll changes. */
  readonly contentY: number
  /** The scroll position then: `contentY - scrollTop` is where its top was on screen. */
  readonly scrollTop: number
  /** How far below its day's top it lies, for where its day is not drawn. */
  readonly inDay: number
}

export interface KeepPlaceOptions {
  readonly scroller: Signal<ElementRef<HTMLElement>>
  /** The days the list shows, top to bottom. */
  readonly days: Signal<readonly TimelineDay[]>
  /** Where the virtual list places the top of the day of `index` in the content; null without it. */
  readonly startOf: (index: number) => number | null
  /** The days drawn: what moves when they are measured, grow or rewrap. */
  readonly sections: Signal<readonly ElementRef<HTMLElement>[]>
  /** How far below the scroller's top the reader reads: just under the sticky day header. */
  readonly line: () => number
}

export interface KeptPlace extends ScrollPlaces {
  /**
   * The virtual list changed what it draws or where (a day measured, the range moved): the anchor
   * is kept after the render that follows, before it is painted. A resize the list's own
   * measuring causes reaches a `ResizeObserver` of the list only with the next frame.
   */
  changed(): void
}

/**
 * Keeps the reader's place in the Timeline, as a browser's scroll anchoring would, which the
 * virtual list has turned off: the row read stays where it is on screen whatever changes above
 * it. Days measured once drawn, rows a collection adds or removes, a filter and a window that
 * rewraps the rows all leave it in place.
 *
 * The row read is the first one whose top is at or below the line under the sticky day header
 * (its day, where no row is there), read again from the document whenever the user scrolls, and
 * held as its top in the scroller's content. After every render of the days and every resize of
 * the list or a drawn day, the list is scrolled by as much as that top moved, measured where it is
 * drawn, or at its day's estimated place until its day is drawn; a scroll the user made meanwhile
 * stays. A row a filter or a collection takes away hands over to the next row of the list
 * that stays, then to its day. At the top of the list nothing is kept, so the newest days a
 * collection brings come into view.
 *
 * Back and Forward come back to the row through `ScrollPlaces.save`, as long as it is there.
 */
export function keepPlace(options: KeepPlaceOptions): KeptPlace {
  const { scroller, days, startOf, sections, line } = options
  let anchor: Anchor | null = null
  /** The days the anchor was read among, to find the row that takes over a gone one's place. */
  let readAmong: readonly TimelineDay[] = []
  /** The scroll position this wrote last, until the user scrolls away from it. */
  let written: number | null = null

  /** Where each entry and day is: the index of its day. */
  const dayIndexes = computed(() => {
    const byKey = new Map<EntryKey | DayKey, number>()
    days().forEach((day, index) => {
      byKey.set(day.date, index)
      for (const row of day.rows) byKey.set(row.entry.trackKey, index)
    })
    return byKey
  })

  const root = (): HTMLElement => scroller().nativeElement
  const screenYOf = (element: Element): number =>
    element.getBoundingClientRect().top - root().getBoundingClientRect().top
  const keyOf = (mark: Mark): EntryKey | DayKey => (mark.kind === 'entry' ? mark.key : mark.day)

  /** The drawn element of a mark, if its day is drawn. */
  const elementOf = (mark: Mark): HTMLElement | null => {
    const selector = mark.kind === 'entry' ? '.entry[data-entry]' : 'article[data-day]'
    for (const element of root().querySelectorAll<HTMLElement>(selector)) {
      const key = mark.kind === 'entry' ? element.dataset['entry'] : element.dataset['day']
      if (key === keyOf(mark)) return element
    }
    return null
  }

  const anchorAt = (element: HTMLElement, mark: Mark): Anchor => {
    const day = element.closest('article')
    const screenY = screenYOf(element)
    const { scrollTop } = root()
    return {
      mark,
      contentY: screenY + scrollTop,
      scrollTop,
      inDay: day === null ? 0 : screenY - screenYOf(day),
    }
  }

  /** The first drawn row at or below the line, else the day across it; null at the top. */
  const readAnchor = (): Anchor | null => {
    const element = root()
    if (!element.isConnected || element.scrollTop <= 0) return null
    const lineY = line()
    const rows = [...element.querySelectorAll<HTMLElement>('.entry[data-entry]')]
    let low = 0
    let high = rows.length
    while (low < high) {
      const middle = (low + high) >> 1
      if (screenYOf(rows[middle]!) >= lineY) high = middle
      else low = middle + 1
    }
    const row = rows[low]
    const key = row === undefined ? null : parseEntryKey(row.dataset['entry'] ?? '')
    if (row !== undefined && key !== null) return anchorAt(row, { kind: 'entry', key })
    for (const article of element.querySelectorAll<HTMLElement>('article[data-day]')) {
      const day = article.dataset['day'] ?? ''
      const top = screenYOf(article)
      if (isDayKey(day) && top <= lineY && top + article.offsetHeight > lineY) {
        return anchorAt(article, { kind: 'day', day })
      }
    }
    return null
  }

  /**
   * The anchor in the days on screen: its own mark, or where a filter or a collection took its
   * row away, the next row of the list it was read among that stays, then its day; null without.
   */
  const heirOf = (held: Anchor): Anchor | null => {
    const present = dayIndexes()
    if (present.has(keyOf(held.mark))) return held
    if (held.mark.kind !== 'entry') return null
    const gone = held.mark.key
    const rows = readAmong.flatMap(day => day.rows.map(row => row.entry.trackKey))
    const heir = rows.slice(rows.indexOf(gone) + 1).find(candidate => present.has(candidate))
    if (heir !== undefined) return { ...held, mark: { kind: 'entry', key: heir }, inDay: 0 }
    const day = readAmong.find(shown => shown.rows.some(row => row.entry.trackKey === gone))
    return day !== undefined && present.has(day.date)
      ? { ...held, mark: { kind: 'day', day: day.date }, inDay: 0 }
      : null
  }

  /** Where the anchor is in the content now: drawn, or where the virtual list puts its day. */
  const contentYOf = (held: Anchor): number | null => {
    const element = elementOf(held.mark)
    if (element !== null) return screenYOf(element) + root().scrollTop
    const index = dayIndexes().get(keyOf(held.mark))
    const start = index === undefined ? null : startOf(index)
    return start === null ? null : start + held.inDay
  }

  /** Scrolls to `target`, and says where the anchor is from now on. */
  const scrollTo = (held: Anchor, now: number, target: number): Anchor => {
    const element = root()
    if (Math.abs(target - element.scrollTop) >= 0.5) {
      element.scrollTop = target
      written = element.scrollTop
    }
    return { ...held, contentY: now, scrollTop: element.scrollTop }
  }

  /**
   * Scrolls by as much as the anchor moved in the content, so a scroll the user made meanwhile
   * stays. Where the list shrank below the scroll position, the browser cut it back to the end:
   * that is no scroll of the user's, and the anchor goes back to where it was on screen.
   */
  const follow = (held: Anchor): Anchor | null => {
    const now = contentYOf(held)
    if (now === null) return null
    const element = root()
    const end = element.scrollHeight - element.clientHeight
    const cut = element.scrollTop !== held.scrollTop && element.scrollTop >= end - 1
    const from = cut ? held.scrollTop : element.scrollTop
    return scrollTo(held, now, from + now - held.contentY)
  }

  /** Puts a saved anchor back where it was on screen. */
  const restore = (saved: Anchor): Anchor | null => {
    const now = contentYOf(saved)
    return now === null ? null : scrollTo(saved, now, now - (saved.contentY - saved.scrollTop))
  }

  const keep = (): void => {
    if (!root().isConnected) {
      anchor = null
      return
    }
    const held = anchor === null ? null : heirOf(anchor)
    readAmong = untracked(days)
    anchor = held === null ? null : follow(held)
  }

  // The user's scrolling reads the anchor again. The position this wrote last is its own, also
  // for every scroll event that reports it again: only a scroll away from it is the user's.
  listener.passive(scroller, 'scroll', () => {
    if (written !== null && Math.abs(root().scrollTop - written) < 1) return
    written = null
    anchor = readAnchor()
    readAmong = untracked(days)
  })

  // After every render of the days, other days or other rows: what was drawn may have moved it.
  // Hand-written: the drawn days change with the list, and Signality's `resizeObserver` takes a
  // fixed list of targets. A day measured, grown by its rows or rewrapped moves what is below it.
  afterRenderEffect(onCleanup => {
    days()
    const drawn = sections()
    untracked(keep)
    const observer = new ResizeObserver(() => keep())
    const list = root().querySelector('.timeline')
    if (list !== null) observer.observe(list)
    for (const section of drawn) observer.observe(section.nativeElement)
    onCleanup(() => observer.disconnect())
  })

  const injector = inject(Injector)
  let afterRender = false

  return {
    changed: (): void => {
      if (afterRender) return
      afterRender = true
      afterNextRender(
        {
          write: () => {
            afterRender = false
            keep()
          },
        },
        { injector }
      )
    },
    save: (): (() => boolean) | null => {
      const saved = readAnchor()
      if (saved === null) return null
      return () => {
        const placed = dayIndexes().has(keyOf(saved.mark)) ? restore(saved) : null
        if (placed === null) return false
        // Kept from here: where its day was not drawn, the next render puts it exactly.
        anchor = placed
        readAmong = untracked(days)
        return true
      }
    },
  }
}
