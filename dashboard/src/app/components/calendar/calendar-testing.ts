// Fakes of what the calendar views read, for their specs.

import { computed, signal, type Provider, type WritableSignal } from '@angular/core'
import type { EntryKey } from '../../providers/connection'
import type { EntryUiAdapter } from '../../providers/types'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivitySelection, type RangeCoverage } from '../../services/activity-selection.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import type { ViewMode } from '../../utils/app-url'
import { DEFAULT_HOUR_HEIGHT } from '../../utils/calendar-zoom'
import { addDaysToKey, dateOfKey, type DayKey } from '../../utils/local-day'
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
}

export interface FakeCalendarState {
  readonly providers: Provider[]
  readonly selectedDay: WritableSignal<DayKey>
  readonly currentView: WritableSignal<ViewMode>
  readonly calendarEntries: WritableSignal<readonly EntryUiAdapter[]>
  readonly hourHeight: WritableSignal<number>
  /** The events and groups the view opened, by key. */
  readonly opened: (EntryKey | readonly EntryKey[])[]
  /** How often the view collected the range on screen. */
  readonly collects: () => number
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
  let collects = 0
  const data = {
    collectShownRange: (): Promise<void> => {
      collects += 1
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
    opened,
    collects: () => collects,
  }
}
