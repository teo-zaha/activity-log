import { describe, expect, it } from 'vitest'
import {
  anchoredScrollTop,
  clampHourHeight,
  hourHeightSerializer,
  stepHourHeight,
  zoomPercent,
} from './calendar-zoom'

describe('calendar zoom', () => {
  it('steps to the next level, also from between levels', () => {
    expect(stepHourHeight(64, 1)).toBe(80)
    expect(stepHourHeight(64, -1)).toBe(48)
    expect(stepHourHeight(70, 1)).toBe(80)
    expect(stepHourHeight(70, -1)).toBe(64)
  })

  it('stays at the ends', () => {
    expect(stepHourHeight(192, 1)).toBe(192)
    expect(stepHourHeight(32, -1)).toBe(32)
    expect(clampHourHeight(500)).toBe(192)
    expect(clampHourHeight(1)).toBe(32)
  })

  it('shows the scale against the default', () => {
    expect(zoomPercent(64)).toBe('100%')
    expect(zoomPercent(96)).toBe('150%')
    expect(zoomPercent(32)).toBe('50%')
  })

  it('keeps the time under the anchor in place', () => {
    // 40 px of all-day row, scrolled to 9:00 at 64 px per hour, anchor 200 px down the view.
    const gridTop = 40
    const scrollTop = gridTop + 9 * 64
    const anchorY = 200
    const timeAtAnchor = (top: number, hour: number): number => (top + anchorY - gridTop) / hour

    const next = anchoredScrollTop(scrollTop, gridTop, anchorY, 64, 128)

    expect(timeAtAnchor(next, 128)).toBeCloseTo(timeAtAnchor(scrollTop, 64))
  })

  it('never scrolls above the top', () => {
    expect(anchoredScrollTop(0, 0, 300, 128, 32)).toBe(0)
  })

  it('reads a stored zoom through the clamp, and anything else as the default', () => {
    expect(hourHeightSerializer.read('500')).toBe(192)
    expect(hourHeightSerializer.read('80')).toBe(80)
    expect(hourHeightSerializer.read('wide')).toBe(64)
    expect(hourHeightSerializer.write(96)).toBe('96')
  })
})
