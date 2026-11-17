/** A month heading on the day index rail: `Nov`, or `’25` where the year changes. */
export interface DayIndexMonth {
  readonly kind: 'month'
  readonly key: string
  readonly label: string
}

/** One calendar day on the day index rail, with or without events. */
export interface DayIndexDay {
  readonly kind: 'day'
  /** `YYYY-MM-DD` */
  readonly key: string
  readonly count: number
  readonly dayOfMonth: number
  readonly isWeekend: boolean
  readonly isToday: boolean
  readonly isMonday: boolean
  /** `count` relative to the busiest day in the range, 0 to 1. */
  readonly weight: number
}

export type DayIndexItem = DayIndexMonth | DayIndexDay

/** How much the rail can label at its current pixels per day. */
export type DayIndexDensity = 'full' | 'sparse' | 'dense'
