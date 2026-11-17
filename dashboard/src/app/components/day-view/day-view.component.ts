import { Component, ElementRef, afterNextRender, computed, inject, viewChild } from '@angular/core'
import { ActivityService } from '../../services/activity.service'
import type { EntryUiAdapter } from '../../providers/types'
import { injectBreakpointDown } from '../../utils/breakpoints'
import { dateOfKey, dayKeyOf, todayKey } from '../../utils/local-day'
import { calendarEntryKey, layoutEventsWithOverflow } from '../calendar-layout'

export const HOUR_START = 0
export const HOUR_END = 23

const DEFAULT_SCROLL_HOUR = 6.5 // 06:30
const ALLDAY_BAR_HEIGHT = 26
const ALLDAY_BAR_GAP = 2
const ALLDAY_BAR_SIDE_GAP = 3
const EVENT_GAP = 4

interface EventStyleVars {
  readonly left: string
  readonly width: string
  readonly right: string
}

interface PositionedDayEvent {
  readonly kind: 'event'
  readonly key: string
  readonly entry: EntryUiAdapter
  readonly top: number
  readonly height: number
  readonly style: EventStyleVars
}

interface PositionedDayOverflow {
  readonly kind: 'overflow'
  readonly key: string
  readonly entries: EntryUiAdapter[]
  readonly top: number
  readonly height: number
  readonly style: EventStyleVars
}

interface PositionedAllDayEntry {
  readonly key: string
  readonly entry: EntryUiAdapter
  readonly top: number
  readonly height: number
  readonly left: string
  readonly width: string
  readonly right: string
}

@Component({
  selector: 'app-day-view',
  templateUrl: './day-view.component.html',
  styleUrl: './day-view.component.scss',
})
export class DayViewComponent {
  protected readonly activity = inject(ActivityService)
  private readonly isPhone = injectBreakpointDown('phone')

  private readonly scrollEl = viewChild.required<ElementRef<HTMLElement>>('scrollEl')

  protected readonly hours = Array.from(
    { length: HOUR_END - HOUR_START + 1 },
    (_, i) => HOUR_START + i
  )

  constructor() {
    afterNextRender({
      write: () => {
        this.scrollEl().nativeElement.scrollTop = DEFAULT_SCROLL_HOUR * this.activity.hourHeight()
      },
    })
  }

  protected gridHeight(): number {
    return (HOUR_END - HOUR_START + 1) * this.activity.hourHeight()
  }

  protected formatDayHeader(dateStr: string): string {
    const compact = this.isPhone()
    return dateOfKey(dateStr).toLocaleDateString('en-GB', {
      weekday: compact ? 'short' : 'long',
      day: 'numeric',
      month: compact ? 'short' : 'long',
      year: 'numeric',
    })
  }

  protected isToday(dateStr: string): boolean {
    return dateStr === todayKey()
  }

  protected getTop(ts: string): number {
    const d = new Date(ts)
    const h = this.activity.hourHeight()
    return Math.max(0, (d.getHours() - HOUR_START) * h + (d.getMinutes() / 60) * h)
  }

  protected getHeight(entry: EntryUiAdapter): number {
    const durationMinutes = entry.calendarDurationMinutes
    if (durationMinutes != null) {
      return Math.max(22, (durationMinutes / 60) * this.activity.hourHeight())
    }
    return 22
  }

  private isMultiDayEvent(e: EntryUiAdapter): boolean {
    if (e.source !== 'outlook' && e.source !== 'calendar') return false
    const durationMinutes = e.calendarDurationMinutes
    if (durationMinutes == null) return false
    const endMs = new Date(e.timestamp).getTime() + durationMinutes * 60000
    return dayKeyOf(endMs) > e.dayKey
  }

  readonly multiDayEntries = computed<EntryUiAdapter[]>(() => {
    const day = this.activity.selectedDay()
    return this.activity.filteredEntries().filter(e => {
      if (!this.isMultiDayEvent(e)) return false
      const endMs = new Date(e.timestamp).getTime() + (e.calendarDurationMinutes ?? 0) * 60000
      return e.dayKey <= day && dayKeyOf(endMs) > day
    })
  })

  protected readonly allDayEntries = computed<PositionedAllDayEntry[]>(() =>
    this.multiDayEntries().map((entry, index) => ({
      key: calendarEntryKey(entry),
      entry,
      left: `${ALLDAY_BAR_SIDE_GAP}px`,
      right: `${ALLDAY_BAR_SIDE_GAP}px`,
      width: `calc(100% - ${ALLDAY_BAR_SIDE_GAP * 2}px)`,
      top: index * (ALLDAY_BAR_HEIGHT + ALLDAY_BAR_GAP) + ALLDAY_BAR_GAP,
      height: ALLDAY_BAR_HEIGHT,
    }))
  )

  protected allDayRowHeight(): number {
    const count = this.multiDayEntries().length
    return count === 0 ? 0 : count * (ALLDAY_BAR_HEIGHT + ALLDAY_BAR_GAP) + ALLDAY_BAR_GAP * 2
  }

  protected readonly dayLayout = computed<(PositionedDayEvent | PositionedDayOverflow)[]>(() => {
    const layout = layoutEventsWithOverflow(
      this.activity.dayEntries().filter(e => !this.isMultiDayEvent(e)),
      entry => this.getTop(entry.timestamp),
      entry => this.getHeight(entry),
      22,
      5
    )

    return layout.map(item => {
      const style = this.eventStyle(item.col, item.cols)
      if (item.kind === 'overflow') {
        return {
          kind: 'overflow',
          key: `overflow:${item.entries.map(calendarEntryKey).join('|')}`,
          entries: item.entries,
          top: item.top,
          height: item.height,
          style,
        }
      }

      return {
        kind: 'event',
        key: calendarEntryKey(item.entry),
        entry: item.entry,
        top: this.getTop(item.entry.timestamp),
        height: this.getHeight(item.entry),
        style,
      }
    })
  })

  private eventStyle(col: number, cols: number): EventStyleVars {
    const pct = 100 / cols
    return {
      left: `calc(${col * pct}% + ${EVENT_GAP}px)`,
      width: `calc(${pct}% - ${EVENT_GAP * 2}px)`,
      right: 'unset',
    }
  }

  protected zoomOut(): void {
    this.activity.hourHeight.update(h => Math.max(32, h - 16))
  }
  protected zoomIn(): void {
    this.activity.hourHeight.update(h => Math.min(192, h + 16))
  }

  protected selectEntry(entry: EntryUiAdapter): void {
    this.activity.selectedEntries.set(null)
    this.activity.selectedEntry.set(entry)
  }

  protected selectOverflow(entries: EntryUiAdapter[]): void {
    this.activity.selectedEntry.set(null)
    this.activity.selectedEntries.set(entries)
  }

  protected formatHour(h: number): string {
    return `${String(h).padStart(2, '0')}:00`
  }

  protected eventAriaLabel(entry: EntryUiAdapter): string {
    return `Open event: ${entry.title}`
  }

  protected overflowAriaLabel(entries: readonly EntryUiAdapter[]): string {
    return `Open ${entries.length} additional events`
  }
}
