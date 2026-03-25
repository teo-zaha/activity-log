import { HttpClient } from '@angular/common/http'
import { Injectable, computed, inject, signal } from '@angular/core'
import { firstValueFrom } from 'rxjs'
import type { ActivityEntry, ActivityLog } from '../generated/activity'
import type { DayGroup } from '../models/day-group'
import type { ExportedActivityLog } from '../models/exported-activity-log'
import { getEntryType } from '../utils/activity-entry'
import { formatDuration, getEntrySearchTerms } from '../utils/activity-presentation'

export type ViewMode = 'list' | 'day' | 'workweek' | 'week'

@Injectable({ providedIn: 'root' })
export class ActivityService {
  private readonly http = inject(HttpClient)

  readonly allEntries = signal<ActivityEntry[]>([])
  readonly meta = signal<ActivityLog['meta'] | null>(null)
  readonly dateFrom = signal('')
  readonly dateTo = signal('')
  readonly searchQuery = signal('')
  readonly activeSources = signal(new Set<string>(['git', 'gitlab', 'jira', 'outlook', 'system']))
  readonly isLoading = signal(false)
  readonly error = signal<string | null>(null)

  // View state
  readonly currentView = signal<ViewMode>('list')
  readonly selectedDay = signal<string>(this.todayStr())
  readonly selectedEntry = signal<ActivityEntry | null>(null)
  readonly selectedEntries = signal<ActivityEntry[] | null>(null)
  readonly hourHeight = signal(64)

  // ── Computed: filtered list ───────────────────────────────────────────────

  readonly filteredEntries = computed(() => {
    const entries = this.allEntries()
    const from = this.dateFrom()
    const to = this.dateTo()
    const query = this.currentView() === 'list' ? this.searchQuery().toLowerCase() : ''
    const sources = this.activeSources()

    return entries.filter(e => {
      const ts = e.timestamp?.slice(0, 10) ?? ''
      if (from && ts < from) return false
      if (to && ts > to) return false
      const isMerged = e.source === 'git' && !!e.gitlabEvent
      if (isMerged ? !sources.has('git') && !sources.has('gitlab') : !sources.has(e.source))
        return false
      if (query) {
        const haystack = [...getEntrySearchTerms(e), getEntryType(e)]
          .filter(Boolean)
          .join(' ')
          .toLowerCase()
        if (!haystack.includes(query)) return false
      }
      return true
    })
  })

  readonly groupedByDay = computed<DayGroup[]>(() => {
    const byDay = new Map<string, ActivityEntry[]>()
    for (const e of this.filteredEntries()) {
      const day = e.timestamp?.slice(0, 10) ?? 'unknown'
      if (!byDay.has(day)) byDay.set(day, [])
      byDay.get(day)!.push(e)
    }
    return Array.from(byDay.entries())
      .sort(([a], [b]) => b.localeCompare(a))
      .map(([date, entries]) => ({ date, entries }))
  })

  // ── Computed: calendar views ──────────────────────────────────────────────

  readonly weekDays = computed<string[]>(() => {
    const start = this.mondayOf(this.selectedDay())
    return Array.from({ length: 7 }, (_, i) => {
      const d = new Date(start + 'T12:00:00')
      d.setDate(d.getDate() + i)
      return d.toISOString().slice(0, 10)
    })
  })

  readonly workWeekDays = computed<string[]>(() => this.weekDays().slice(0, 5))

  readonly dayEntries = computed(() =>
    this.filteredEntries().filter(e => e.timestamp.slice(0, 10) === this.selectedDay())
  )

  readonly weekEntriesByDay = computed<Map<string, ActivityEntry[]>>(() => {
    const days = new Set(this.weekDays())
    const map = new Map<string, ActivityEntry[]>()
    for (const day of days) map.set(day, [])
    for (const e of this.filteredEntries()) {
      const day = e.timestamp.slice(0, 10)
      if (map.has(day)) map.get(day)!.push(e)
    }
    return map
  })

  // ── Navigation ────────────────────────────────────────────────────────────

  navigateDay(delta: number): void {
    const d = new Date(this.selectedDay() + 'T12:00:00')
    d.setDate(d.getDate() + delta)
    this.selectedDay.set(d.toISOString().slice(0, 10))
  }

  navigateWeek(delta: number): void {
    this.navigateDay(delta * 7)
  }

  goToToday(): void {
    this.selectedDay.set(this.todayStr())
  }

  // ── Data loading ──────────────────────────────────────────────────────────

  loadLog(log: ActivityLog): void {
    this.allEntries.set(log.entries)
    this.meta.set(log.meta)
    if (log.meta.since) this.dateFrom.set(log.meta.since.slice(0, 10))
    if (log.meta.until) this.dateTo.set(log.meta.until.slice(0, 10))
  }

  setView(view: ViewMode): void {
    this.currentView.set(view)
    this.searchQuery.set('')
  }

  toggleSource(source: string): void {
    const current = new Set(this.activeSources())
    if (current.has(source)) current.delete(source)
    else current.add(source)
    this.activeSources.set(current)
  }

  isolateSource(source: string): void {
    const current = this.activeSources()
    if (current.size === 1 && current.has(source)) {
      this.activeSources.set(new Set(['git', 'gitlab', 'jira', 'outlook', 'system']))
    } else {
      this.activeSources.set(new Set([source]))
    }
  }

  async initLoad(): Promise<void> {
    this.isLoading.set(true)
    this.error.set(null)
    try {
      for (let i = 0; i < 60; i++) {
        const res = await firstValueFrom(
          this.http.get<ActivityLog | { collecting: boolean }>('/api/activity')
        )
        if ('collecting' in res) {
          await new Promise(r => setTimeout(r, 1000))
          continue
        }
        this.loadLog(res)
        return
      }
      this.error.set('Timed out waiting for initial collection.')
    } catch (e) {
      this.error.set(`Failed to load: ${String(e)}`)
    } finally {
      this.isLoading.set(false)
    }
  }

  async collect(since: string, until: string): Promise<void> {
    this.isLoading.set(true)
    this.error.set(null)
    try {
      const log = await firstValueFrom(
        this.http.post<ActivityLog>('/api/collect', { since, until })
      )
      this.loadLog(log)
    } catch (e) {
      this.error.set(`Collection failed: ${String(e)}`)
    } finally {
      this.isLoading.set(false)
    }
  }

  exportJson(): string {
    const payload: ExportedActivityLog = {
      meta: this.meta(),
      entries: this.filteredEntries(),
    }
    return JSON.stringify(payload, null, 2)
  }

  exportMarkdown(): string {
    const from = this.dateFrom() || 'all'
    const to = this.dateTo() || 'all'
    let md = `# Activity Summary — ${from} to ${to}\n\nTotal: ${this.filteredEntries().length} events\n\n`
    for (const { date, entries } of this.groupedByDay()) {
      const label = new Date(date + 'T12:00:00Z').toLocaleDateString('en-GB', {
        weekday: 'long',
        year: 'numeric',
        month: 'long',
        day: 'numeric',
      })
      md += `## ${label}\n\n`
      for (const e of entries) {
        const t = new Date(e.timestamp).toLocaleTimeString('en-GB', {
          hour: '2-digit',
          minute: '2-digit',
        })
        if (e.source === 'git') md += `- **[git]** \`${e.repo}\` ${e.message} (${t})\n`
        else if (e.source === 'gitlab') md += `- **[gitlab]** ${e.type}: ${e.title || ''} (${t})\n`
        else if (e.source === 'jira')
          md += `- **[jira]** ${e.key}: ${e.title} [${e.status}] (${t})\n`
        else if (e.source === 'outlook')
          md += `- **[outlook]** ${e.title}${e.location ? ` @ ${e.location}` : ''} (${t}, ${formatDuration(e.duration_minutes)})\n`
        else if (e.source === 'system') md += `- **[system]** ${e.label} (${t})\n`
      }
      md += '\n'
    }
    return md
  }

  // ── Helpers ───────────────────────────────────────────────────────────────

  private todayStr(): string {
    return new Date().toISOString().slice(0, 10)
  }

  private mondayOf(dateStr: string): string {
    const d = new Date(dateStr + 'T12:00:00')
    const day = d.getDay()
    d.setDate(d.getDate() - (day === 0 ? 6 : day - 1))
    return d.toISOString().slice(0, 10)
  }
}
