import { type EnvironmentProviders, Injectable, makeEnvironmentProviders } from '@angular/core'
import {
  DateAdapter,
  MAT_DATE_FORMATS,
  MAT_DATE_LOCALE,
  MAT_NATIVE_DATE_FORMATS,
  NativeDateAdapter,
} from '@angular/material/core'

/**
 * `17/05/2026`, `17.5.2026` or `17-05-26`: day, month, year, as the pickers show dates; or
 * `17,5,2026`, as a phone's keypad for digits (`inputmode="decimal"`) types in a language whose
 * decimal sign is a comma.
 */
const DAY_MONTH_YEAR = /^(\d{1,2})[./,-](\d{1,2})[./,-](\d{2}|\d{4})$/
const YEAR_MONTH_DAY = /^(\d{4})-(\d{1,2})-(\d{1,2})$/
/** `3 May 2026` and `3 May, 2026`. */
const DAY_NAME_YEAR = /^(\d{1,2})\.?\s+([a-z]+)\.?,?\s+(\d{4})$/i
/** `May 3, 2026` and `May 3 2026`. */
const NAME_DAY_YEAR = /^([a-z]+)\.?\s+(\d{1,2}),?\s+(\d{4})$/i

const MONTHS = [
  'january',
  'february',
  'march',
  'april',
  'may',
  'june',
  'july',
  'august',
  'september',
  'october',
  'november',
  'december',
] as const

/**
 * The native adapter, reading typed dates itself: digits day first, as the pickers show them
 * (`en-GB`), and an English month name (`May 3, 2026`, `3 May 2026`). Anything else, a time or
 * a zone after the date included, is no date: the browser's own parser reads digits month first.
 */
@Injectable()
export class DayFirstDateAdapter extends NativeDateAdapter {
  override parse(value: unknown, parseFormat?: unknown): Date | null {
    if (typeof value !== 'string') return super.parse(value, parseFormat)
    const text = value.trim()
    if (text === '') return null
    const dayFirst = DAY_MONTH_YEAR.exec(text)
    if (dayFirst) return this.dateOf(fullYear(dayFirst[3]), Number(dayFirst[2]), dayFirst[1])
    const iso = YEAR_MONTH_DAY.exec(text)
    if (iso) return this.dateOf(Number(iso[1]), Number(iso[2]), iso[3])
    const dayName = DAY_NAME_YEAR.exec(text)
    if (dayName) return this.dateOf(Number(dayName[3]), monthNumber(dayName[2]), dayName[1])
    const nameDay = NAME_DAY_YEAR.exec(text)
    if (nameDay) return this.dateOf(Number(nameDay[3]), monthNumber(nameDay[1]), nameDay[2])
    return this.invalid()
  }

  /** The local day, or an invalid date for one the calendar does not have (31/02). */
  private dateOf(year: number, month: number | null, day: string): Date {
    if (month === null) return this.invalid()
    const date = new Date(year, month - 1, Number(day))
    date.setFullYear(year)
    return date.getMonth() === month - 1 && date.getDate() === Number(day) ? date : this.invalid()
  }
}

/** Two-digit years are this century's. */
function fullYear(year: string): number {
  return year.length === 2 ? 2000 + Number(year) : Number(year)
}

/** 1–12 for a month's name or its first three letters (`Sep` and `Sept` too); null otherwise. */
function monthNumber(name: string): number | null {
  const word = name.toLowerCase()
  const index = MONTHS.findIndex(
    month => month === word || (word.length >= 3 && month.startsWith(word))
  )
  return index === -1 ? null : index + 1
}

/** The date pickers' adapter, formats and locale, for the whole app. */
export function provideDashboardDates(): EnvironmentProviders {
  return makeEnvironmentProviders([
    { provide: DateAdapter, useClass: DayFirstDateAdapter },
    { provide: MAT_DATE_FORMATS, useValue: MAT_NATIVE_DATE_FORMATS },
    { provide: MAT_DATE_LOCALE, useValue: 'en-GB' },
  ])
}
