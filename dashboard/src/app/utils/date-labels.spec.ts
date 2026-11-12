import { describe, expect, it } from 'vitest'
import { localDateKey, relativeDayLabel } from './date-labels'

describe('date labels', () => {
  it('formats local date keys', () => {
    expect(localDateKey(new Date(2026, 0, 5, 23, 30))).toBe('2026-01-05')
  })

  it('labels today and yesterday, including across month boundaries', () => {
    expect(relativeDayLabel('2026-03-01', '2026-03-01')).toBe('Today')
    expect(relativeDayLabel('2026-02-28', '2026-03-01')).toBe('Yesterday')
    expect(relativeDayLabel('2026-02-27', '2026-03-01')).toBeNull()
  })
})
