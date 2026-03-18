import { Component, inject, AfterViewInit, ElementRef, ViewChild } from '@angular/core'
import { NgStyle } from '@angular/common'
import { ActivityService } from '../../services/activity.service'
import type { ActivityEntry, OutlookEntry } from '../../models/activity'
import { isMergedEntry } from '../../models/activity'
import { layoutEventsWithOverflow, type CalendarItem } from '../calendar-layout'

export const HOUR_START = 0
export const HOUR_END   = 23

const DEFAULT_SCROLL_HOUR = 6.5 // 06:30

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
      return Math.max(22, ((e as OutlookEntry).duration_minutes / 60) * this.activity.hourHeight())
    }
    return 22
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

  protected dayLayout(): CalendarItem<ActivityEntry>[] {
    return layoutEventsWithOverflow(
      this.activity.dayEntries(),
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
