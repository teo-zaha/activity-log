import { signal } from '@angular/core'
import type { ComponentFixture } from '@angular/core/testing'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { BackendFailure } from '../../services/backend/backend-failure'
import { failure, failureOf } from '../../services/failure'
import { ActivityDataService, type SourceLink } from '../../services/activity-data.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import type { CollectionStatus } from '../../services/types'
import { CollectionStatusComponent, STATUS_CLEARANCE } from './collection-status.component'
import { stubLayoutObservers } from '../../testing/observers'
import type { DayKey } from '../../utils/local-day'

/** This tab's range and the collection's, 17–18 May both unless a spec says otherwise. */
interface Ranges {
  readonly own?: { from: DayKey; to: DayKey }
  readonly collected?: { from: DayKey; to: DayKey } | null
}

const MAY = { from: '2026-05-17', to: '2026-05-18' } as const

async function setup(
  status: CollectionStatus,
  collecting: readonly string[] = [],
  link: SourceLink = { status: { kind: 'open' }, since: 0, episode: 0 },
  ranges: Ranges = {}
): Promise<{
  retryCollection: ReturnType<typeof vi.fn>
  dismissCollectionStatus: ReturnType<typeof vi.fn>
  collectShownRange: ReturnType<typeof vi.fn>
  fixture: ComponentFixture<CollectionStatusComponent>
}> {
  stubLayoutObservers()
  const retryCollection = vi.fn(() => Promise.resolve())
  const dismissCollectionStatus = vi.fn()
  const collectShownRange = vi.fn(() => Promise.resolve())
  const own = ranges.own ?? MAY
  const activity = {
    collectionStatus: signal(status),
    providers: signal(
      ['Git', 'System'].map(label => ({ label, collecting: collecting.includes(label) }))
    ),
    retryCollection,
    dismissCollectionStatus,
    collectShownRange,
    connection: signal(link),
  }
  const viewState = {
    dateFrom: signal(own.from),
    dateTo: signal(own.to),
    collectedRange: signal(ranges.collected === undefined ? MAY : ranges.collected),
  }

  const { fixture } = await render(CollectionStatusComponent, {
    providers: [
      { provide: ActivityDataService, useValue: activity },
      // The real `ActivitySelection` decides how the collection covers this tab's range.
      { provide: ActivityViewStateService, useValue: viewState },
    ],
  })

  return { retryCollection, dismissCollectionStatus, collectShownRange, fixture }
}

describe('CollectionStatusComponent', () => {
  it('renders nothing while idle', async () => {
    await setup({ kind: 'idle' })

    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByText(/collecting/i)).toBeNull()
  })

  it('announces progress with the sources being collected', async () => {
    await setup({ kind: 'collecting', operation: 'collect', startedAt: Date.now() })

    const status = screen.getByRole('status')
    expect(status.textContent).toContain('Collecting activity')
    expect(status.textContent).toContain('From Git, System')
  })

  it('names the sources it still waits for once some are in', async () => {
    await setup({ kind: 'collecting', operation: 'collect', startedAt: Date.now() }, ['System'])

    const status = screen.getByRole('status')
    expect(status.textContent).toContain('Collecting activity')
    expect(status.textContent).toContain('Still collecting System')
    expect(status.textContent).not.toContain('Git')
  })

  it('says what the collection on screen covers where it leaves out this tab’s range', async () => {
    const { collectShownRange } = await setup({ kind: 'idle' }, [], undefined, {
      own: { from: '2026-05-01', to: '2026-05-18' },
      collected: { from: '2026-04-01', to: '2026-04-08' },
    })

    const note = screen.getByRole('status')
    expect(note.textContent).toContain('1 Apr – 8 Apr 2026')
    await userEvent.click(screen.getByRole('button', { name: 'Collect 1 May – 18 May 2026' }))
    expect(collectShownRange).toHaveBeenCalledOnce()
  })

  it('says nothing of a collection that covers this tab’s range', async () => {
    await setup({ kind: 'idle' }, [], undefined, {
      own: { from: '2026-05-17', to: '2026-05-17' },
      collected: { from: '2026-05-01', to: '2026-05-31' },
    })

    expect(screen.queryByRole('button', { name: /^Collect/ })).toBeNull()
    expect(screen.getByRole('status').textContent?.trim()).toBe('')
  })

  it('tells the notices how much of the window’s bottom its card takes, while it shows', async () => {
    const status = signal<CollectionStatus>({
      kind: 'failed',
      operation: 'collect',
      origin: 'collection',
      failure: failure('GitLab unreachable'),
    })
    stubLayoutObservers()
    const root = document.documentElement
    const { fixture } = await render(CollectionStatusComponent, {
      providers: [
        {
          provide: ActivityDataService,
          useValue: {
            collectionStatus: status,
            providers: signal([]),
            retryCollection: vi.fn(),
            dismissCollectionStatus: vi.fn(),
            collectShownRange: vi.fn(),
            connection: signal({ status: { kind: 'open' }, since: 0, episode: 0 }),
          },
        },
        {
          provide: ActivityViewStateService,
          useValue: {
            dateFrom: signal(MAY.from),
            dateTo: signal(MAY.to),
            collectedRange: signal(MAY),
          },
        },
      ],
    })
    await fixture.whenStable()
    // jsdom lays nothing out, so the distance is no real one; it is set.
    expect(root.style.getPropertyValue(STATUS_CLEARANCE)).toMatch(/^\d+px$/)

    status.set({ kind: 'idle' })
    await fixture.whenStable()
    expect(root.style.getPropertyValue(STATUS_CLEARANCE)).toBe('')

    status.set({ kind: 'collecting', operation: 'collect', startedAt: Date.now() })
    await fixture.whenStable()
    expect(root.style.getPropertyValue(STATUS_CLEARANCE)).toMatch(/^\d+px$/)
    fixture.destroy()
    expect(root.style.getPropertyValue(STATUS_CLEARANCE)).toBe('')
  })

  it('shows a reconnect over the state, and says when it is out of date', async () => {
    vi.useFakeTimers({ toFake: ['Date', 'setInterval', 'clearInterval'] })
    try {
      vi.setSystemTime(10_000)
      const reconnecting: SourceLink = {
        status: { kind: 'reconnecting', failure: failure('connection reset') },
        since: 10_000,
        episode: 0,
      }
      const { fixture } = await setup({ kind: 'idle' }, [], reconnecting)
      expect(screen.getByRole('status').textContent).toContain('Reconnecting')
      expect(screen.getByRole('status').textContent).not.toContain('out of date')

      vi.advanceTimersByTime(5000)
      await fixture.whenStable()
      expect(screen.getByRole('status').textContent).toContain('may be out of date')
    } finally {
      vi.useRealTimers()
    }
  })

  it('shows a lost backend with Retry and Dismiss', async () => {
    const { retryCollection, dismissCollectionStatus } = await setup({
      kind: 'failed',
      operation: 'initial',
      origin: 'lostBackend',
      failure: failure('offline'),
    })

    const alert = screen.getByRole('alert').textContent
    expect(alert).toContain('Could not load activity')
    expect(alert).toContain('offline')
    await userEvent.click(screen.getByRole('button', { name: /retry/i }))
    await userEvent.click(screen.getByRole('button', { name: 'Dismiss' }))
    expect(retryCollection).toHaveBeenCalled()
    expect(dismissCollectionStatus).toHaveBeenCalled()
  })

  it('offers no retry for a request the backend refused as invalid', async () => {
    const refused = new BackendFailure('invalidRequest', 'A range can span at most 10 years')
    await setup({
      kind: 'failed',
      operation: 'collect',
      origin: 'collect',
      failure: failureOf(refused),
    })

    expect(screen.getByRole('alert').textContent).toContain('at most 10 years')
    expect(screen.queryByRole('button', { name: /retry/i })).toBeNull()
    expect(screen.getByRole('button', { name: 'Dismiss' })).toBeTruthy()
  })

  it('offers retry for a request that may work when sent again', async () => {
    const busy = new BackendFailure('internal', 'The collection could not start')
    await setup({
      kind: 'failed',
      operation: 'collect',
      origin: 'collect',
      failure: failureOf(busy),
    })

    expect(screen.getByRole('button', { name: /retry/i })).toBeTruthy()
  })

  it('offers no retry for a state read the backend refuses for good', async () => {
    const missing = new BackendFailure('notFound', '404 Not Found')
    await setup({
      kind: 'failed',
      operation: 'initial',
      origin: 'refusedRead',
      failure: failureOf(missing),
    })

    expect(screen.getByRole('alert').textContent).toContain('404 Not Found')
    expect(screen.queryByRole('button', { name: /retry/i })).toBeNull()
  })
})
