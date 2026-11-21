import { describe, expect, it } from 'vitest'
import { dayAt, slideLine, type ScrollMetrics } from './active-day'

describe('slideLine', () => {
  // A 600 px screen with the line 42 px down, over a list 2000 px tall: the last 558 px slide.
  const list: Omit<ScrollMetrics, 'scrollTop'> = { height: 600, line: 42, scrollHeight: 2000 }

  it('keeps the line in place at the top and until the last stretch', () => {
    expect(slideLine({ ...list, scrollTop: 0 })).toBe(0)
    expect(slideLine({ ...list, scrollTop: 842 })).toBe(0)
  })

  it('slides it to the bottom edge at the end of the list', () => {
    expect(slideLine({ ...list, scrollTop: 1400 })).toBe(557)
    expect(42 + slideLine({ ...list, scrollTop: 1400 })).toBe(list.height - 1)
  })

  it('slides it in proportion over the last stretch', () => {
    expect(slideLine({ ...list, scrollTop: 1400 - 279 })).toBeCloseTo(278.5)
  })

  it('stays at the bottom edge while the list bounces past its end', () => {
    expect(slideLine({ ...list, scrollTop: 1450 })).toBe(557)
  })

  it('slides over the whole overflow of a list barely taller than the screen', () => {
    const short = { ...list, scrollHeight: 700 }

    expect(slideLine({ ...short, scrollTop: 0 })).toBe(0)
    expect(slideLine({ ...short, scrollTop: 50 })).toBeCloseTo(278.5)
    expect(slideLine({ ...short, scrollTop: 100 })).toBe(557)
  })

  it('does not slide a list that does not scroll', () => {
    expect(slideLine({ ...list, scrollHeight: 600, scrollTop: 0 })).toBe(0)
  })
})

describe('dayAt', () => {
  const bottoms = [100, 300, 600]
  const bottomOf = (index: number): number => bottoms[index] ?? 0

  it('is the first day above the first section', () => {
    expect(dayAt(3, bottomOf, -10)).toBe(0)
  })

  it('is the section that reaches below the line', () => {
    expect(dayAt(3, bottomOf, 50)).toBe(0)
    expect(dayAt(3, bottomOf, 100)).toBe(1)
    expect(dayAt(3, bottomOf, 599)).toBe(2)
  })

  it('is the last day past the last section', () => {
    expect(dayAt(3, bottomOf, 1000)).toBe(2)
  })

  it('is none without sections', () => {
    expect(dayAt(0, bottomOf, 0)).toBe(-1)
  })

  it('reads few sections of a long list', () => {
    let reads = 0
    const index = dayAt(
      1000,
      i => {
        reads++
        return (i + 1) * 100
      },
      54_321
    )

    expect(index).toBe(543)
    expect(reads).toBeLessThanOrEqual(11)
  })
})
