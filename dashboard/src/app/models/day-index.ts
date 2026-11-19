/** A month heading on the day index rail: `Nov`, or `Dec ’25` where the year changes. */
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

/** How much a rail of months can label at its pixels per month, see `monthDensityFor`. */
export type MonthDensity = 'months' | 'months-compact' | 'quarters' | 'years'

/** How much the rail can label at its current pixels per day; the month densities show months. */
export type DayIndexDensity = 'full' | 'sparse' | 'dense' | MonthDensity

/** One month of a rail too short for days: its row stands for all of its days in the range. */
export interface DayIndexMonthRow {
  /** `YYYY-MM` */
  readonly key: string
  /** `Nov` */
  readonly label: string
  /** `’25` on the newest month of an older year, when the range spans years; else null. */
  readonly year: string | null
  /** Days of the month inside the range. */
  readonly days: number
  readonly count: number
  /** `count` relative to the busiest month in the range, 0 to 1. */
  readonly weight: number
}
