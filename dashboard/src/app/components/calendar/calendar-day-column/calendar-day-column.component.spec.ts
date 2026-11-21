import { render, screen } from '@testing-library/angular'
import { describe, expect, it, vi } from 'vitest'
import type { EntryKey } from '../../../providers/connection'
import { createEntryAdapter } from '../../../providers/provider-adapter'
import type { EntryUiAdapter } from '../../../providers/types'
import { layoutDays, nowMarker, planCalendar } from '../calendar-grid'
import { CalendarDayColumnComponent } from './calendar-day-column.component'

function meeting(id: string, time: string): EntryUiAdapter {
  return createEntryAdapter({
    source: 'calendar',
    connection: 'team',
    id,
    timestamp: `2026-11-17T${time}:00+01:00`,
    title: id,
    durationMinutes: 60,
    meetingWeight: 'busy',
  })
}

describe('CalendarDayColumnComponent', () => {
  // Three at once in a column two wide: one block and an overflow of two.
  const entries = [
    meeting('Planning', '09:00'),
    meeting('Review', '09:15'),
    meeting('Sync', '09:30'),
  ]
  const [column] = layoutDays(planCalendar(['2026-11-17'], entries, 'Europe/Berlin'), 64, 2)

  it('opens the event of a block and the group of an overflow', async () => {
    const openEvent = vi.fn<(key: EntryKey) => void>()
    const openGroup = vi.fn<(keys: readonly EntryKey[]) => void>()
    await render(CalendarDayColumnComponent, {
      inputs: { column },
      on: { openEvent, openGroup },
    })

    screen.getByRole('button', { name: 'Open event: Planning' }).click()
    screen.getByRole('button', { name: 'Open 2 additional events' }).click()

    expect(openEvent).toHaveBeenCalledWith(entries[0].trackKey)
    expect(openGroup).toHaveBeenCalledWith([entries[1].trackKey, entries[2].trackKey])
  })

  it('draws the current-time line only when given the time', async () => {
    const { container, fixture } = await render(CalendarDayColumnComponent, {
      inputs: { column },
    })
    expect(container.querySelector('.now-line')).toBeNull()

    fixture.componentRef.setInput(
      'now',
      nowMarker(Date.parse('2026-11-17T00:00:00+01:00'), 64, 'Europe/Berlin')
    )
    await fixture.whenStable()

    // Midnight: a line at the very top, not one taken for none.
    expect(container.querySelector<HTMLElement>('.now-line')?.style.top).toBe('0px')
  })
})
