import { describe, expect, it } from 'vitest'
import { calendarEntryKey, layoutEventsWithOverflow } from './calendar-layout'

interface TestEntry {
  id: string
  source: string
  timestamp: string
  title: string
  top: number
  height: number
}

const entry = (id: string, top: number, height = 30): TestEntry => ({
  id,
  source: 'git',
  timestamp: `2026-05-18T09:${id.padStart(2, '0')}:00Z`,
  title: `Entry ${id}`,
  top,
  height,
})

describe('calendar layout', () => {
  it('keeps non-overlapping entries in one column', () => {
    const layout = layoutEventsWithOverflow(
      [entry('1', 0), entry('2', 40)],
      item => item.top,
      item => item.height
    )

    expect(layout).toMatchObject([
      { kind: 'event', col: 0, cols: 1 },
      { kind: 'event', col: 0, cols: 1 },
    ])
  })

  it('assigns overlapping entries to separate columns', () => {
    const layout = layoutEventsWithOverflow(
      [entry('1', 0, 50), entry('2', 10, 50)],
      item => item.top,
      item => item.height
    )

    expect(layout).toMatchObject([
      { kind: 'event', col: 0, cols: 2 },
      { kind: 'event', col: 1, cols: 2 },
    ])
  })

  it('collapses excess overlap columns into overflow', () => {
    const layout = layoutEventsWithOverflow(
      [entry('1', 0, 50), entry('2', 5, 50), entry('3', 10, 50)],
      item => item.top,
      item => item.height,
      22,
      2
    )

    expect(layout.some(item => item.kind === 'overflow')).toBe(true)
  })

  it('creates deterministic entry keys', () => {
    expect(calendarEntryKey(entry('1', 0))).toBe('git:2026-05-18T09:01:00Z:Entry 1:')
  })
})
