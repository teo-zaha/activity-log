import { signal } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { Router, provideRouter } from '@angular/router'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import type { ActivityLog } from '../../generated/activity'
import { ActivityService } from '../../services/activity.service'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'
import { ToastService } from '../../services/toast.service'
import { activityLog } from '../../testing/activity-fixtures'
import { HeaderComponent } from './header.component'

async function setup(state: { loading?: boolean; meta?: ActivityLog['meta'] } = {}): Promise<void> {
  await render(HeaderComponent, {
    providers: [
      provideRouter([]),
      {
        provide: ActivityService,
        useValue: {
          isLoading: signal(state.loading ?? false),
          meta: signal(state.meta ?? null),
        },
      },
    ],
  })
}

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
    url = '/day/2026-11-17?q=standup&settings=logs',
    options: { desktop?: boolean; clipboard?: 'none' | 'denied' } = {}
  ): Promise<{
    writeText: ReturnType<typeof vi.fn>
    success: ReturnType<typeof vi.fn>
    failure: ReturnType<typeof vi.fn>
  }> {
    const writeText = vi.fn(() =>
      options.clipboard === 'denied'
        ? Promise.reject(new DOMException('Write permission denied.', 'NotAllowedError'))
        : Promise.resolve()
    )
    const success = vi.fn()
    const failure = vi.fn()
    // jsdom's navigator has no clipboard of its own: `none` leaves it out.
    const readText = (): Promise<string> => Promise.resolve('')
    vi.stubGlobal('navigator', {
      ...navigator,
      ...nav,
      ...(options.clipboard === 'none' ? {} : { clipboard: { writeText, readText } }),
    })
    await render(HeaderComponent, {
      providers: [
        provideRouter([{ path: '**', children: [] }]),
        {
          provide: ActivityService,
          useValue: {
            isLoading: signal(false),
            meta: signal(null),
          },
        },
        { provide: ToastService, useValue: { success, failure } },
        {
          provide: RuntimeEnvironmentService,
          useValue: { isTauri: (): boolean => options.desktop ?? true },
        },
      ],
    })
    await TestBed.inject(Router).navigateByUrl(url)
    return { writeText, success, failure }
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

  it('says the link was not copied where there is no clipboard', async () => {
    const { success, failure } = await setupShare({}, undefined, { clipboard: 'none' })

    await userEvent.click(screen.getByRole('button', { name: 'Share link to this view' }))

    expect(success).not.toHaveBeenCalled()
    expect(failure).toHaveBeenCalledWith({
      kind: 'other',
      message: 'Copying the link failed: this browser gives the page no clipboard',
    })
  })

  it('says the link was not copied when the clipboard refuses it', async () => {
    const { writeText, success, failure } = await setupShare({}, undefined, {
      clipboard: 'denied',
    })

    await userEvent.click(screen.getByRole('button', { name: 'Share link to this view' }))

    expect(writeText).toHaveBeenCalledWith(link)
    expect(success).not.toHaveBeenCalled()
    expect(failure).toHaveBeenCalledWith({
      kind: 'other',
      message: 'Copying the link failed: Write permission denied.',
      cause: expect.any(DOMException),
    })
  })

  it('gives both reasons when the system refuses to share and the clipboard to copy', async () => {
    const refusal = new DOMException('Sharing is not allowed here.', 'NotAllowedError')
    const share = vi.fn((_data: ShareData) => Promise.reject(refusal))
    const { success, failure } = await setupShare({ share, canShare: () => true }, undefined, {
      clipboard: 'denied',
    })

    await userEvent.click(screen.getByRole('button', { name: 'Share link to this view' }))

    expect(success).not.toHaveBeenCalled()
    expect(failure).toHaveBeenCalledWith({
      kind: 'other',
      message:
        'Sharing the link failed: Sharing is not allowed here.; ' +
        'Copying the link failed: Write permission denied.',
      cause: [refusal, expect.any(DOMException)],
    })
  })

  it('shares the page’s own address, whole query included, in the browser', async () => {
    const share = vi.fn((_data: ShareData) => Promise.resolve())
    const url =
      '/day/2026-11-17?q=standup&settings=logs&events=git%2Fa%2F1&events=git%2Fa%2F2&event=git%2Fa%2F2'
    const { writeText } = await setupShare({ share, canShare: () => true }, url, {
      desktop: false,
    })

    await userEvent.click(screen.getByRole('button', { name: 'Share link to this view' }))

    const shared = share.mock.calls[0]?.[0]
    expect(shared?.url).toBe(`${document.location.origin}${url}`)
    expect(shared?.text).toBeUndefined()
    expect(writeText).not.toHaveBeenCalled()
  })
})
