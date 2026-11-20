import { describe, expect, it } from 'vitest'
import {
  addDaysToKey,
  clockTime,
  dayKeyOf,
  isDayKey,
  keysWithin,
  lastDayKeyBefore,
  localDateKey,
  localDayRange,
  longDayLabel,
  monthLabel,
  relativeDayLabel,
} from './local-day'

// Expectations are built with local `Date` constructors, so they hold in any time zone.
describe('local days', () => {
  it('accepts only keys of real days', () => {
    expect(isDayKey('2026-11-17')).toBe(true)
    expect(isDayKey('2028-02-29')).toBe(true)
    expect(isDayKey('2026-02-29')).toBe(false)
    expect(isDayKey('2026-1-7')).toBe(false)
    expect(isDayKey('')).toBe(false)
  })

  it('formats local date keys', () => {
    expect(localDateKey(new Date(2026, 0, 5, 23, 30))).toBe('2026-01-05')
  })

  it('reads the local day of an instant', () => {
    expect(dayKeyOf(new Date(2026, 2, 25, 0, 30).toISOString())).toBe('2026-03-25')
    expect(dayKeyOf(new Date(2026, 2, 25, 23, 30).getTime())).toBe('2026-03-25')
  })

  it('steps days across month and year boundaries', () => {
    expect(addDaysToKey('2026-02-28', 1)).toBe('2026-03-01')
    expect(addDaysToKey('2026-01-01', -1)).toBe('2025-12-31')
  })

  it('turns local days into a half-open range of instants and back', () => {
    const range = localDayRange('2026-03-28', '2026-03-29')

    expect(range).toEqual({
      start: new Date(2026, 2, 28).toISOString(),
      end: new Date(2026, 2, 30).toISOString(),
    })
    expect(dayKeyOf(range.start)).toBe('2026-03-28')
    expect(lastDayKeyBefore(range.end)).toBe('2026-03-29')
  })

  it('labels today and yesterday, including across month boundaries', () => {
    expect(relativeDayLabel('2026-03-01', '2026-03-01')).toBe('Today')
    expect(relativeDayLabel('2026-02-28', '2026-03-01')).toBe('Yesterday')
    expect(relativeDayLabel('2026-02-27', '2026-03-01')).toBeNull()
  })
})

describe('labels', () => {
  it('shows the local clock time of an instant', () => {
    const instant = new Date(2026, 10, 16, 9, 5).toISOString()

    expect(clockTime(instant)).toBe('09:05')
  })

  it('shows a day key as a long date', () => {
    expect(longDayLabel('2026-11-16')).toBe('Monday, 16 November 2026')
  })

  it('writes month labels', () => {
    expect(monthLabel('2026-09-01')).toBe('Sep')
  })

  it('keeps the keys within a range, a missing bound not limiting', () => {
    const keys = ['2026-11-16', '2026-11-17', '2026-11-18']
    expect(keysWithin(keys, '2026-11-17', '2026-11-17')).toEqual(['2026-11-17'])
    expect(keysWithin(keys, '2026-11-17', null)).toEqual(['2026-11-17', '2026-11-18'])
    expect(keysWithin(keys, null, null)).toEqual(keys)
  })
})
