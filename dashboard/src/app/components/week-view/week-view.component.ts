import { Component, inject, AfterViewInit, ElementRef, ViewChild } from '@angular/core'
import { NgStyle } from '@angular/common'
import { ActivityService } from '../../services/activity.service'
import type { ActivityEntry, OutlookEntry } from '../../models/activity'
import { isMergedEntry } from '../../models/activity'
import { layoutEventsWithOverflow, type CalendarItem } from '../calendar-layout'

const HOUR_START = 0
const HOUR_END   = 23
const DEFAULT_SCROLL_HOUR = 6.5 // 06:30

@Component({
  selector: 'app-week-view',
  standalone: true,
  imports: [NgStyle],
  templateUrl: './week-view.component.html',
  styleUrl: './week-view.component.scss',
})
export class WeekViewComponent implements AfterViewInit {
  protected readonly activity = inject(ActivityService)
  protected isMerged = isMergedEntry

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

  protected formatWeekRange(days: string[]): string {
    if (!days.length) return ''
    const s = new Date(days[0] + 'T12:00:00')
    const e = new Date(days[days.length - 1] + 'T12:00:00')
    const month = s.getMonth() === e.getMonth()
      ? s.toLocaleDateString('en-GB', { month: 'long', year: 'numeric' })
      : `${s.toLocaleDateString('en-GB', { month: 'short' })} – ${e.toLocaleDateString('en-GB', { month: 'short', year: 'numeric' })}`
    return month
  }

  protected formatDayCol(dateStr: string): { weekday: string; date: string } {
    const d = new Date(dateStr + 'T12:00:00')
    return {
      weekday: d.toLocaleDateString('en-GB', { weekday: 'short' }),
      date: String(d.getDate()),
    }
  }

  protected isToday(dateStr: string): boolean {
    return dateStr === new Date().toISOString().slice(0, 10)
  }

  protected isSelectedDay(dateStr: string): boolean {
    return dateStr === this.activity.selectedDay()
  }

  protected get days(): string[] {
    return this.activity.currentView() === 'workweek'
      ? this.activity.workWeekDays()
      : this.activity.weekDays()
  }

  protected getEntriesForDay(dateStr: string): ActivityEntry[] {
    return this.activity.weekEntriesByDay().get(dateStr) ?? []
  }

  protected getLayoutForDay(dateStr: string): CalendarItem<ActivityEntry>[] {
    const maxVisible = this.activity.currentView() === 'workweek' ? 4 : 3
    return layoutEventsWithOverflow(
      this.getEntriesForDay(dateStr),
      (e: ActivityEntry) => this.getTop(e.timestamp),
      (e: ActivityEntry) => this.getHeight(e),
      22,
      maxVisible,
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

  protected getTop(ts: string): number {
    const d = new Date(ts)
    const h = this.activity.hourHeight()
    return Math.max(0, (d.getHours() - HOUR_START) * h + (d.getMinutes() / 60) * h)
  }

  protected getHeight(e: ActivityEntry): number {
    if (e.source === 'outlook') {
      return Math.max(20, ((e as OutlookEntry).duration_minutes / 60) * this.activity.hourHeight())
    }
    return 20
  }

  protected getShortTitle(e: ActivityEntry): string {
    switch (e.source) {
      case 'git':     return (e as any).message ?? ''
      case 'gitlab':  return (e as any).title || (e as any).type
      case 'jira':  { const j = e as any; return j.key ? `${j.key}: ${j.title ?? ''}` : (j.title ?? '') }
      case 'outlook': return (e as any).title ?? ''
      case 'system':  return (e as any).label ?? e.type
    }
  }

  protected zoomOut(): void { this.activity.hourHeight.update(h => Math.max(32, h - 16)) }
  protected zoomIn():  void { this.activity.hourHeight.update(h => Math.min(192, h + 16)) }

  protected selectDay(dateStr: string): void {
    this.activity.selectedDay.set(dateStr)
    this.activity.setView('day')
  }

  protected selectEntry(e: ActivityEntry): void {
    this.activity.selectedEntries.set(null)
    this.activity.selectedEntry.set(e)
  }

  protected selectOverflow(entries: ActivityEntry[]): void {
    this.activity.selectedEntry.set(null)
    this.activity.selectedEntries.set(entries)
  }

  protected getSourceIcon(source: string, type?: string): string {
    if (source === 'system') {
      const t: Record<string, string> = {
        boot: 'power_settings_new', shutdown: 'power_off',
        unexpected_shutdown: 'warning', sleep: 'bedtime',
        wake: 'wb_sunny', lock: 'lock', unlock: 'lock_open',
      }
      return t[type ?? ''] ?? 'power_settings_new'
    }
    const icons: Record<string, string> = {
      git: 'commit', gitlab: 'code', jira: 'task_alt', outlook: 'event',
    }
    return icons[source] ?? 'circle'
  }

  protected formatHour(h: number): string {
    return `${String(h).padStart(2, '0')}:00`
  }
}
