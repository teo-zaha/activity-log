import { signal } from '@angular/core'
import { MAT_DATE_LOCALE } from '@angular/material/core'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { ActivityService } from '../../services/activity.service'
import { HeaderComponent } from './header.component'

async function setup(): Promise<{
  dateFrom: ReturnType<typeof signal<string>>
  dateTo: ReturnType<typeof signal<string>>
  collect: ReturnType<typeof vi.fn<(since: string, until: string) => Promise<void>>>
}> {
  const dateFrom = signal('2026-05-17')
  const dateTo = signal('2026-05-18')
  const collect = vi.fn<(since: string, until: string) => Promise<void>>(() => Promise.resolve())
  await render(HeaderComponent, {
    providers: [
      {
        provide: ActivityService,
        useValue: {
          dateFrom,
          dateTo,
          collect,
          isLoading: signal(false),
          meta: signal(null),
          filteredEntries: signal([]),
        },
      },
      { provide: MAT_DATE_LOCALE, useValue: 'en-GB' },
    ],
  })
  return { dateFrom, dateTo, collect }
}

describe('HeaderComponent date range', () => {
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
