import { signal } from '@angular/core'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { appConfig } from '../../app.config'
import { ActivityService } from '../../services/activity.service'
import type { DayKey } from '../../utils/local-day'
import { RangeFormComponent } from './range-form.component'

async function setup(): Promise<{
  dateFrom: ReturnType<typeof signal<DayKey | null>>
  dateTo: ReturnType<typeof signal<DayKey | null>>
  collect: ReturnType<typeof vi.fn<(since: string, until: string) => Promise<void>>>
}> {
  const dateFrom = signal<DayKey | null>('2026-05-17')
  const dateTo = signal<DayKey | null>('2026-05-18')
  const collect = vi.fn<(since: string, until: string) => Promise<void>>(() => Promise.resolve())
  await render(RangeFormComponent, {
    providers: [
      {
        provide: ActivityService,
        useValue: {
          dateFrom,
          dateTo,
          setDateFrom: (day: DayKey | null): void => dateFrom.set(day),
          setDateTo: (day: DayKey | null): void => dateTo.set(day),
          collect,
          isLoading: signal(false),
        },
      },
      // The app's own providers: the date adapter is the one the app starts with.
      ...appConfig.providers,
    ],
  })
  return { dateFrom, dateTo, collect }
}

describe('RangeFormComponent', () => {
  it('shows the current range', async () => {
    await setup()

    expect(screen.getByLabelText('From date')).toHaveProperty('value', '17/05/2026')
    expect(screen.getByLabelText('To date')).toHaveProperty('value', '18/05/2026')
  })

  it('applies a typed date once the field is committed', async () => {
    const { dateFrom, dateTo } = await setup()
    const from = screen.getByLabelText('From date')

    await userEvent.clear(from)
    await userEvent.type(from, 'May 3, 2026')
    expect(dateFrom()).toBe('2026-05-17')
    await userEvent.tab()

    expect(dateFrom()).toBe('2026-05-03')
    expect(dateTo()).toBe('2026-05-18')
  })

  it('reads a typed date as day, month, year', async () => {
    const { dateFrom } = await setup()
    const from = screen.getByLabelText('From date')

    await userEvent.clear(from)
    await userEvent.type(from, '03/05/2026')
    await userEvent.tab()
    expect(dateFrom()).toBe('2026-05-03')

    await userEvent.clear(from)
    await userEvent.type(from, '13/05/2026')
    await userEvent.tab()
    expect(dateFrom()).toBe('2026-05-13')
  })

  it('reads a month by its name, and no date from digits with a time or zone after them', async () => {
    const { dateFrom } = await setup()
    const from = screen.getByLabelText('From date')

    await userEvent.clear(from)
    await userEvent.type(from, '3 May 2026')
    await userEvent.tab()
    expect(dateFrom()).toBe('2026-05-03')

    // The browser would read these month first, as 5 March; they are no date.
    for (const pasted of ['03/05/2026 GMT', '03/05/2026 9:00 AM']) {
      await userEvent.clear(from)
      await userEvent.type(from, pasted)
      await userEvent.tab()
      expect(dateFrom()).toBeNull()
    }
  })

  it('collects the range on submit', async () => {
    const { collect } = await setup()

    await userEvent.click(screen.getByRole('button', { name: 'Collect activity' }))

    expect(collect).toHaveBeenCalledWith('2026-05-17', '2026-05-18')
  })

  it('collects with Enter in a date field', async () => {
    const { collect } = await setup()

    await userEvent.type(screen.getByLabelText('To date'), '{Enter}')

    expect(collect).toHaveBeenCalledWith('2026-05-17', '2026-05-18')
  })

  it('still collects when the range is inverted', async () => {
    const { collect } = await setup()
    const from = screen.getByLabelText('From date')

    await userEvent.clear(from)
    await userEvent.type(from, 'May 25, 2026')
    await userEvent.tab()
    await userEvent.click(screen.getByRole('button', { name: 'Collect activity' }))

    expect(collect).toHaveBeenCalledWith('2026-05-25', '2026-05-18')
  })
})
