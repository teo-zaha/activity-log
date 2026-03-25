import { Component, inject, AfterViewInit, ElementRef, ViewChild, computed } from '@angular/core'
import { NgStyle } from '@angular/common'
import { ActivityService } from '../../services/activity.service'
import type { ActivityEntry, OutlookEntry } from '../../generated/activity'
import { isMergedEntry } from '../../utils/activity-entry'
import { getEntryIcon, getEntryTitle } from '../../utils/activity-presentation'
import { layoutEventsWithOverflow, type CalendarItem } from '../calendar-layout'

export const HOUR_START = 0
export const HOUR_END   = 23

const DEFAULT_SCROLL_HOUR = 6.5 // 06:30
const ALLDAY_BAR_HEIGHT = 26
const ALLDAY_BAR_GAP    = 2

@Component({
  selector: 'app-day-view',
  standalone: true,
  imports: [NgStyle],
  templateUrl: './day-view.component.html',
  styleUrl: './day-view.component.scss',
})
export class DayViewComponent implements AfterViewInit {
  protected readonly activity = inject(ActivityService)
  protected isMerged = isMergedEntry
  protected getEntryIcon = getEntryIcon
  protected getEntryTitle = getEntryTitle

  @ViewChild('scrollEl') private scrollEl!: ElementRef<HTMLElement>

  protected readonly hours = Array.from(
    { length: HOUR_END - HOUR_START + 1 },
    (_, i) => HOUR_START + i
  )

  protected gridHeight(): number {
    return (HOUR_END - HOUR_START + 1) * this.activity.hourHeight()
  }

  ngAfterViewInit(): void {
    this.scrollEl.nativeElement.scrollTop = DEFAULT_SCROLL_HOUR * this.activity.hourHeight()
  }

  protected formatDayHeader(dateStr: string): string {
    return new Date(dateStr + 'T12:00:00').toLocaleDateString('en-GB', {
      weekday: 'long', day: 'numeric', month: 'long', year: 'numeric',
    })
  }

  protected isToday(dateStr: string): boolean {
    return dateStr === new Date().toISOString().slice(0, 10)
  }

  protected getTop(ts: string): number {
    const d = new Date(ts)
    const h = this.activity.hourHeight()
    return Math.max(0, (d.getHours() - HOUR_START) * h + (d.getMinutes() / 60) * h)
  }

  protected getHeight(e: ActivityEntry): number {
    if (e.source === 'outlook') {
      return Math.max(22, (e.duration_minutes / 60) * this.activity.hourHeight())
    }
    return 22
  }

  private isMultiDayEvent(e: ActivityEntry): e is OutlookEntry {
    if (e.source !== 'outlook') return false
    const endMs = new Date(e.timestamp).getTime() + (e as OutlookEntry).duration_minutes * 60000
    return new Date(endMs).toISOString().slice(0, 10) > e.timestamp.slice(0, 10)
  }

  readonly multiDayEntries = computed(() => {
    const day = this.activity.selectedDay()
    return this.activity.filteredEntries().filter(e => {
      if (!this.isMultiDayEvent(e)) return false
      const startDay = e.timestamp.slice(0, 10)
      const endMs = new Date(e.timestamp).getTime() + (e as OutlookEntry).duration_minutes * 60000
      const endDay = new Date(endMs).toISOString().slice(0, 10)
      return startDay <= day && endDay > day
    }) as OutlookEntry[]
  })

  protected allDayRowHeight(): number {
    const count = this.multiDayEntries().length
    return count === 0 ? 0 : count * (ALLDAY_BAR_HEIGHT + ALLDAY_BAR_GAP) + ALLDAY_BAR_GAP * 2
  }

  protected getDayAllDayBarStyle(index: number): Record<string, string> {
    const GAP = 3
    return {
      left: `${GAP}px`,
      right: `${GAP}px`,
      width: `calc(100% - ${GAP * 2}px)`,
      top: `${index * (ALLDAY_BAR_HEIGHT + ALLDAY_BAR_GAP) + ALLDAY_BAR_GAP}px`,
      height: `${ALLDAY_BAR_HEIGHT}px`,
    }
  }

  protected dayLayout(): CalendarItem<ActivityEntry>[] {
    return layoutEventsWithOverflow(
      this.activity.dayEntries().filter(e => !this.isMultiDayEvent(e)),
      e => this.getTop(e.timestamp),
      e => this.getHeight(e),
      22,
      5,
    )
  }

  protected getEventStyle(col: number, cols: number): Record<string, string> {
    const GAP = 4
    const pct = 100 / cols
    return {
      left: `calc(${col * pct}% + ${GAP}px)`,
      width: `calc(${pct}% - ${GAP * 2}px)`,
      right: 'unset',
    }
  }

  protected zoomOut(): void { this.activity.hourHeight.update(h => Math.max(32, h - 16)) }
  protected zoomIn():  void { this.activity.hourHeight.update(h => Math.min(192, h + 16)) }

  protected selectEntry(e: ActivityEntry): void {
    this.activity.selectedEntries.set(null)
    this.activity.selectedEntry.set(e)
  }

  protected selectOverflow(entries: ActivityEntry[]): void {
    this.activity.selectedEntry.set(null)
    this.activity.selectedEntries.set(entries)
  }

  protected formatHour(h: number): string {
    return `${String(h).padStart(2, '0')}:00`
  }
}
