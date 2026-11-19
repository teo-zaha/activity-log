import { signal } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { Router, provideRouter } from '@angular/router'
import { MAT_DATE_LOCALE } from '@angular/material/core'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ActivityLog } from '../../generated/activity'
import { ActivityService } from '../../services/activity.service'
import { ToastService } from '../../services/toast.service'
import { activityLog } from '../../testing/activity-fixtures'
import { HeaderComponent } from './header.component'

async function setup(state: { loading?: boolean; meta?: ActivityLog['meta'] } = {}): Promise<{
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
          setDateFrom: (day: string): void => dateFrom.set(day),
          setDateTo: (day: string): void => dateTo.set(day),
          collect,
          isLoading: signal(state.loading ?? false),
          meta: signal(state.meta ?? null),
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

describe('HeaderComponent export', () => {
  const exportButtons = (): HTMLElement[] =>
    ['Export Markdown', 'Export JSON'].map(name => screen.getByRole('button', { name }))

  it('is offered for a collected log', async () => {
    await setup({ meta: activityLog.meta })

    expect(exportButtons().map(button => button.hasAttribute('disabled'))).toEqual([false, false])
  })

  it('waits until a running collection is complete', async () => {
    await setup({ meta: activityLog.meta, loading: true })

    expect(exportButtons().map(button => button.hasAttribute('disabled'))).toEqual([true, true])
  })
})

describe('HeaderComponent share', () => {
  const link = 'activity-log://day/2026-11-17?q=standup'

  async function setupShare(
    nav: {
      share?: (data: ShareData) => Promise<void>
      canShare?: (data?: ShareData) => boolean
    },
    url = '/day/2026-11-17?q=standup&settings=logs'
  ): Promise<{ writeText: ReturnType<typeof vi.fn>; success: ReturnType<typeof vi.fn> }> {
    const writeText = vi.fn(() => Promise.resolve())
    const success = vi.fn()
    vi.stubGlobal('navigator', {
      ...navigator,
      ...nav,
      clipboard: { writeText, readText: () => Promise.resolve('') },
    })
    await render(HeaderComponent, {
      providers: [
        provideRouter([{ path: '**', children: [] }]),
        {
          provide: ActivityService,
          useValue: {
            dateFrom: signal('2026-11-17'),
            dateTo: signal('2026-11-18'),
            isLoading: signal(false),
            meta: signal(null),
          },
        },
        { provide: ToastService, useValue: { success } },
        { provide: MAT_DATE_LOCALE, useValue: 'en-GB' },
      ],
    })
    await TestBed.inject(Router).navigateByUrl(url)
    return { writeText, success }
  }

  afterEach(() => {
    vi.unstubAllGlobals()
  })

  it('shares the deep link of the view, without the settings dialog, where it can', async () => {
    const share = vi.fn((_data: ShareData) => Promise.resolve())
    // As WebKit: a custom-scheme `url` cannot be shared, text can.
    const canShare = (data?: ShareData): boolean => !data?.url?.startsWith('activity-log:')
    const { writeText, success } = await setupShare({ share, canShare })

    await userEvent.click(screen.getByRole('button', { name: 'Share link to this view' }))

    expect(share).toHaveBeenCalledWith({ title: 'Activity Log', text: link })
    expect(writeText).not.toHaveBeenCalled()
    expect(success).not.toHaveBeenCalled()
  })

  it('includes the details open over the view', async () => {
    const { writeText } = await setupShare(
      {},
      '/day/2026-11-17?settings=logs&events=git%2Fa%2F1&events=git%2Fa%2F2&event=git%2Fa%2F2'
    )

    await userEvent.click(screen.getByRole('button', { name: 'Share link to this view' }))

    expect(writeText).toHaveBeenCalledWith(
      'activity-log://day/2026-11-17?event=git%2Fa%2F2&events=git%2Fa%2F1&events=git%2Fa%2F2'
    )
  })

  it('copies the link and says so where the system cannot share it', async () => {
    const { writeText, success } = await setupShare({})

    await userEvent.click(screen.getByRole('button', { name: 'Share link to this view' }))

    expect(writeText).toHaveBeenCalledWith(link)
    expect(success).toHaveBeenCalledWith('Link copied')
  })
})
