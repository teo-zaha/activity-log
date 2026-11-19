import { Clipboard } from '@angular/cdk/clipboard'
import { TestBed } from '@angular/core/testing'
import { render, screen, within } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { LogEntry } from '../../../generated/activity'
import { MockBackendTransport, provideMockBackend } from '../../../testing/activity-fixtures'
import { LOGS_POLL_INTERVAL_MS, SettingsLogsPaneComponent } from './settings-logs-pane.component'

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

async function setup(pollIntervalMs = 60_000, entries = logs): Promise<MockBackendTransport> {
  const backend = new MockBackendTransport()
  backend.logs = [...entries]
  await render(SettingsLogsPaneComponent, {
    providers: [
      provideMockBackend(backend),
      { provide: LOGS_POLL_INTERVAL_MS, useValue: pollIntervalMs },
    ],
  })
  await screen.findByText('Collection failed')
  return backend
}

function messages(): string[] {
  const list = screen.getByRole('log')
  return Array.from(list.querySelectorAll('.log-message')).map(node => node.textContent ?? '')
}

describe('SettingsLogsPaneComponent', () => {
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
    const backend = await setup()
    await userEvent.click(screen.getByRole('button', { name: /Clear/ }))

    expect(backend.logs).toEqual([])
    expect(within(screen.getByRole('log')).getByText(/Nothing logged yet/)).toBeTruthy()
  })

  it('copies the visible entries as text', async () => {
    await setup()
    const clipboard = TestBed.inject(Clipboard)
    const copy = vi.spyOn(clipboard, 'copy').mockReturnValue(true)

    await userEvent.click(screen.getByRole('button', { name: /Copy/ }))

    expect(copy).toHaveBeenCalledWith(expect.stringContaining('ERROR [outlook] Collection failed'))
    expect(await screen.findByRole('button', { name: /Copied/ })).toBeTruthy()
  })

  it('keeps the last entries and shows the error when a poll fails', async () => {
    const backend = await setup(20)
    backend.logsError = new Error('backend went away')

    expect(await screen.findByRole('alert')).toHaveProperty(
      'textContent',
      expect.stringContaining('backend went away')
    )
    expect(messages()).toContain('Collection failed')
  })

  it('drops a failed clear error once a later poll succeeds', async () => {
    const backend = await setup(40)
    backend.clearLogsError = new Error('disk is read-only')

    await userEvent.click(screen.getByRole('button', { name: /Clear/ }))
    expect((await screen.findByRole('alert')).textContent).toContain('disk is read-only')

    await vi.waitFor(() => expect(screen.queryByRole('alert')).toBeNull())
    expect(messages()).toContain('Collection failed')
  })
})
