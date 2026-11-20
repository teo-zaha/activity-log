import { Clipboard } from '@angular/cdk/clipboard'
import { type ComponentFixture, TestBed } from '@angular/core/testing'
import { render, screen, within } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import type { LogEntry } from '../../../generated/activity'
import { MockBackendTransport, provideMockBackend } from '../../../testing/activity-fixtures'
import { LOGS_POLL_INTERVAL_MS, SettingsLogsPaneComponent } from './settings-logs-pane.component'
import { BackendFailure } from '../../../services/backend/backend-failure'

const logs: LogEntry[] = [
  {
    id: 1,
    timestamp: '2026-11-03T08:00:00Z',
    level: 'info',
    scope: 'git',
    message: 'Scanning 2 repositories',
  },
  {
    id: 2,
    timestamp: '2026-11-03T08:00:01Z',
    level: 'warn',
    scope: 'git',
    message: 'Skipped C:\\code: no user.email configured',
  },
  {
    id: 3,
    timestamp: '2026-11-03T08:00:02Z',
    level: 'error',
    scope: 'outlook',
    message: 'Collection failed',
  },
]

const POLL_MS = 2000

/**
 * Renders the pane over `entries`. The poll's timer is fake: `poll()` fires it once and waits
 * until the pane has shown the answer, so no test waits on the clock.
 */
async function setup(entries = logs): Promise<{
  backend: MockBackendTransport
  poll: () => Promise<void>
  fixture: ComponentFixture<SettingsLogsPaneComponent>
}> {
  const backend = new MockBackendTransport()
  backend.logs = [...entries]
  const { fixture } = await render(SettingsLogsPaneComponent, {
    providers: [provideMockBackend(backend), { provide: LOGS_POLL_INTERVAL_MS, useValue: POLL_MS }],
  })
  await fixture.whenStable()
  expect(screen.getByText('Collection failed')).toBeTruthy()
  const poll = async (): Promise<void> => {
    vi.advanceTimersByTime(POLL_MS)
    await fixture.whenStable()
  }
  return { backend, poll, fixture }
}

function messages(): string[] {
  const list = screen.getByRole('log')
  return Array.from(list.querySelectorAll('.log-message')).map(node => node.textContent ?? '')
}

describe('SettingsLogsPaneComponent', () => {
  // Only the poll's interval: the pane's other timers and Testing Library's run as usual.
  beforeEach(() => vi.useFakeTimers({ toFake: ['setInterval', 'clearInterval'] }))
  afterEach(() => vi.useRealTimers())

  it('shows entries newest first', async () => {
    await setup()
    expect(messages()).toEqual([
      'Collection failed',
      'Skipped C:\\code: no user.email configured',
      'Scanning 2 repositories',
    ])
  })

  it('filters by minimum level', async () => {
    await setup()
    await userEvent.click(screen.getByRole('radio', { name: /Warnings/ }))
    expect(messages()).toEqual(['Collection failed', 'Skipped C:\\code: no user.email configured'])

    await userEvent.click(screen.getByRole('radio', { name: /Errors/ }))
    expect(messages()).toEqual(['Collection failed'])
  })

  it('clears the backend log', async () => {
    const { backend } = await setup()
    await userEvent.click(screen.getByRole('button', { name: /Clear/ }))

    expect(backend.logs).toEqual([])
    expect(within(screen.getByRole('log')).getByText(/Nothing logged yet/)).toBeTruthy()
  })

  it('copies the visible entries as text', async () => {
    const { fixture } = await setup()
    const clipboard = TestBed.inject(Clipboard)
    const copy = vi.spyOn(clipboard, 'copy').mockReturnValue(true)

    await userEvent.click(screen.getByRole('button', { name: /Copy/ }))

    expect(copy).toHaveBeenCalledWith(expect.stringContaining('ERROR [outlook] Collection failed'))
    await fixture.whenStable()
    expect(screen.getByRole('button', { name: /Copied/ })).toBeTruthy()
  })

  it('keeps the last entries and shows the error when a poll fails', async () => {
    const { backend, poll } = await setup()
    backend.logsError = new BackendFailure('internal', 'backend went away')

    await poll()

    expect(screen.getByRole('alert').textContent).toContain('backend went away')
    expect(messages()).toContain('Collection failed')
  })

  it('drops a failed clear error once a later poll succeeds', async () => {
    const { backend, poll, fixture } = await setup()
    backend.clearLogsError = new BackendFailure('internal', 'disk is read-only')

    await userEvent.click(screen.getByRole('button', { name: /Clear/ }))
    await fixture.whenStable()
    expect(screen.getByRole('alert').textContent).toContain('disk is read-only')

    await poll()

    expect(screen.queryByRole('alert')).toBeNull()
    expect(messages()).toContain('Collection failed')
  })

  it('logs the failed clear’s cause beside the message it shows', async () => {
    const { backend, fixture } = await setup()
    const refused = new BackendFailure('internal', 'disk is read-only')
    backend.clearLogsError = refused
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => undefined)

    await userEvent.click(screen.getByRole('button', { name: /Clear/ }))
    await fixture.whenStable()

    expect(screen.getByRole('alert').textContent).toContain('disk is read-only')
    expect(warn).toHaveBeenCalledWith('disk is read-only', refused)
  })
})
