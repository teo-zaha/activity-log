// Fakes of what the calendar views read, for their specs.

import { computed, signal, type Provider, type WritableSignal } from '@angular/core'
import type { ConnectionKey, EntryKey } from '../../providers/connection'
import type { EntryUiAdapter } from '../../providers/types'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection, type RangeCoverage } from '../../services/activity-selection.service'
import { ActivityViewStateService, type DayRange } from '../../services/activity-view-state.service'
import type { CollectionStatus } from '../../services/types'
import type { ViewMode } from '../../utils/app-url'
import { DEFAULT_HOUR_HEIGHT } from '../../utils/calendar-zoom'
import { addDaysToKey, dateOfKey, type DayKey } from '../../utils/local-day'
import { calendarHintText, type CalendarEmptyHint } from './calendar-empty-hint'
import { CALENDAR_TIME_ZONE } from './calendar-time-zone'

export interface CalendarStateInit {
  readonly day: DayKey
  readonly entries?: readonly EntryUiAdapter[]
  readonly view?: ViewMode
  /** This tab's range; none by default. */
  readonly from?: DayKey | null
  readonly to?: DayKey | null
  readonly coverage?: RangeCoverage
  /** The zone the calendar reads clocks in; the runner's own by default. */
  readonly timeZone?: string
  /** The collection on screen; idle by default. */
  readonly status?: CollectionStatus
}

export interface FakeCalendarState {
  readonly providers: Provider[]
  readonly selectedDay: WritableSignal<DayKey>
  readonly currentView: WritableSignal<ViewMode>
  readonly calendarEntries: WritableSignal<readonly EntryUiAdapter[]>
  readonly hourHeight: WritableSignal<number>
  readonly isLoading: WritableSignal<boolean>
  readonly collectionStatus: WritableSignal<CollectionStatus>
  /** Whether any source is usable or still being checked; true by default. */
  readonly anySourceReady: WritableSignal<boolean>
  /** The connections the source filter shows; one by default. */
  readonly activeConnections: WritableSignal<Set<ConnectionKey>>
  /** The events and groups the view opened, by key. */
  readonly opened: (EntryKey | readonly EntryKey[])[]
  /** The ranges the view collected as this tab's own (`ActivityDataService.collectRange`). */
  readonly collected: readonly DayRange[]
}

/**
 * `ActivityViewStateService`, `ActivitySelection` and `ActivityDataService` as far as the calendar
 * views use them.
 */
export function fakeCalendarState(init: CalendarStateInit): FakeCalendarState {
  const selectedDay = signal(init.day)
  const currentView = signal<ViewMode>(init.view ?? 'week')
  const calendarEntries = signal<readonly EntryUiAdapter[]>(init.entries ?? [])
  const hourHeight = signal(DEFAULT_HOUR_HEIGHT)
  const isLoading = signal(false)
  const collectionStatus = signal<CollectionStatus>(init.status ?? { kind: 'idle' })
  const anySourceReady = signal(true)
  const activeConnections = signal(new Set<ConnectionKey>(['calendar/default']))
  const opened: (EntryKey | readonly EntryKey[])[] = []
  const weekDays = computed(() => {
    const day = selectedDay()
    const monday = addDaysToKey(day, -((dateOfKey(day).getDay() + 6) % 7))
    return Array.from({ length: 7 }, (_, i) => addDaysToKey(monday, i))
  })
  const viewState = {
    currentView,
    selectedDay,
    weekDays,
    workWeekDays: computed(() => weekDays().slice(0, 5)),
    dateFrom: signal(init.from ?? null),
    dateTo: signal(init.to ?? null),
    hourHeight,
    activeConnections,
    navigateDay: (delta: number): void => selectedDay.update(day => addDaysToKey(day, delta)),
    navigateWeek: (delta: number): void => selectedDay.update(day => addDaysToKey(day, delta * 7)),
    goToToday: (): void => undefined,
    openDay: (day: DayKey): Promise<boolean> => {
      selectedDay.set(day)
      return Promise.resolve(true)
    },
    openEvent: (key: EntryKey): Promise<boolean> => {
      opened.push(key)
      return Promise.resolve(true)
    },
    openGroup: (keys: readonly EntryKey[]): Promise<boolean> => {
      opened.push(keys)
      return Promise.resolve(true)
    },
  } satisfies Partial<ActivityViewStateService>
  const collected: DayRange[] = []
  const data = {
    isLoading,
    collectionStatus,
    anySourceReady,
    collectRange: (range: DayRange): Promise<void> => {
      collected.push(range)
      return Promise.resolve()
    },
  } satisfies Partial<ActivityDataService>
  const selection = {
    calendarEntries,
    rangeCoverage: signal(init.coverage ?? { kind: 'unknown' }),
  } satisfies Partial<ActivitySelection>
  return {
    providers: [
      { provide: ActivityViewStateService, useValue: viewState },
      { provide: ActivitySelection, useValue: selection },
      { provide: ActivityDataService, useValue: data },
      { provide: CALENDAR_TIME_ZONE, useValue: init.timeZone },
    ],
    selectedDay,
    currentView,
    calendarEntries,
    hourHeight,
    isLoading,
    collectionStatus,
    anySourceReady,
    activeConnections,
    opened,
    collected,
  }
}

/** The words of the hint on screen, in its live region; null without a hint. */
export function shownHint(): { readonly title: string; readonly message: string | null } | null {
  const status = document.querySelector('app-calendar-hint [role="status"]')
  if (status === null) return null
  return {
    title: status.querySelector('.hint-title')?.textContent?.trim() ?? '',
    message: status.querySelector('.hint-message')?.textContent?.trim() ?? null,
  }
}

/** The words `hint` shows, to compare `shownHint()` with: specs name the case, not its text. */
export function hintWords(hint: CalendarEmptyHint): {
  readonly title: string
  readonly message: string | null
} {
  const { title, message } = calendarHintText(hint)
  return { title, message }
}
