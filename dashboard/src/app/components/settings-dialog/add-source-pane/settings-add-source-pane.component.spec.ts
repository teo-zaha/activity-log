import { signal, type WritableSignal } from '@angular/core'
import { render, screen, within } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type {
  ConnectionId,
  InstallReceipt,
  ProviderConfigSnapshotDto,
  ProviderKindDto,
} from '../../../generated/activity'
import type { ReceiptStanding } from '../../../services/types'
import { ActivityDataService } from '../../../services/activity-data.service'
import { BackendFailure } from '../../../services/backend/backend-failure'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
import { ConnectionSettings } from '../../../services/connection-settings.service'
import { providerInfo } from '../../../testing/activity-fixtures'
import { SettingsAddSourcePaneComponent } from './settings-add-source-pane.component'

const kind = (
  overrides: Partial<ProviderKindDto> & Pick<ProviderKindDto, 'source' | 'label'>
): ProviderKindDto => ({
  description: `What ${overrides.label} collects`,
  multiple: true,
  supportedOs: ['windows', 'macos', 'linux'],
  supported: true,
  wslDistros: [],
  ...overrides,
})

const snapshot = (
  connections: ConnectionId[],
  wslDistros: string[] = []
): ProviderConfigSnapshotDto => ({
  revision: 'r1',
  kinds: [
    kind({ source: 'git', label: 'Git', wslDistros }),
    kind({ source: 'gitlab', label: 'GitLab' }),
    kind({ source: 'github', label: 'GitHub' }),
    kind({ source: 'system', label: 'System', multiple: false }),
    kind({
      source: 'outlook',
      label: 'Outlook',
      multiple: false,
      supported: false,
      supportedOs: ['windows', 'macos'],
    }),
    kind({ source: 'figma', label: 'Figma' }),
  ],
  connections: connections.map(connection => ({
    connection,
    label: connection.name,
    fields: [],
  })),
})

async function setup(
  before: ProviderConfigSnapshotDto,
  after: ProviderConfigSnapshotDto,
  desktop = true,
  created: ConnectionId | undefined = after.connections.at(-1)?.connection,
  installed: InstallReceipt | null = { epoch: 'mock-host', revision: 1 },
  firstRead?: () => Promise<ProviderConfigSnapshotDto>
): Promise<{
  addConnection: ReturnType<typeof vi.fn>
  opened: ReturnType<typeof vi.fn>
  loadProviderConfig: ReturnType<typeof vi.fn>
  hostEpoch: WritableSignal<string | null>
}> {
  const addConnection = vi.fn().mockResolvedValue({
    created,
    snapshot: after,
    providersResponse: {
      currentOs: 'windows',
      secretStore: { store: 'os', state: 'ready' },
      providers: providerInfo,
    },
    installed,
  })
  const opened = vi.fn()
  const loadProviderConfig = vi.fn((): Promise<ProviderConfigSnapshotDto> =>
    Promise.resolve(before)
  )
  if (firstRead) loadProviderConfig.mockImplementationOnce(firstRead)
  const hostEpoch = signal<string | null>('mock-host')
  // As the coordinator decides it: no receipt is a save the host never installed.
  const receiptStanding = (receipt: InstallReceipt | null): ReceiptStanding =>
    receipt === null ? 'notInstalled' : 'current'
  await render(SettingsAddSourcePaneComponent, {
    on: { opened },
    providers: [
      {
        provide: ConnectionSettings,
        useValue: { loadProviderConfig, addConnection },
      },
      { provide: ActivityDataService, useValue: { receiptStanding, hostEpoch } },
      { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => desktop } },
    ],
  })
  return { addConnection, opened, loadProviderConfig, hostEpoch }
}

describe('SettingsAddSourcePaneComponent', () => {
  it('adds a source from its tile and opens it', async () => {
    const { addConnection, opened } = await setup(
      snapshot([]),
      snapshot([{ source: 'gitlab', name: 'gitlab' }])
    )

    await userEvent.click(await screen.findByRole('button', { name: 'GitLab' }))

    expect(addConnection).toHaveBeenCalledWith({ source: 'gitlab' })
    expect(opened).toHaveBeenCalledWith('gitlab/gitlab')
  })

  it('opens nothing for an add written but never installed (B70), and reads the snapshot again', async () => {
    const before = snapshot([])
    const { opened, loadProviderConfig } = await setup(
      before,
      snapshot([{ source: 'gitlab', name: 'gitlab' }]),
      true,
      { source: 'gitlab', name: 'gitlab' },
      null
    )
    await screen.findByRole('button', { name: 'GitLab' })
    const reads = loadProviderConfig.mock.calls.length

    await userEvent.click(screen.getByRole('button', { name: 'GitLab' }))

    expect(opened).not.toHaveBeenCalled()
    await vi.waitFor(() => expect(loadProviderConfig.mock.calls.length).toBe(reads + 1))
    // What shows is the snapshot read again, not the answer's.
    expect((await screen.findByRole('button', { name: 'GitLab' })).textContent).not.toContain(
      '1 added'
    )
  })

  it('reads the snapshot again when the host restarts while it is read', async () => {
    let answer = (_: ProviderConfigSnapshotDto): void => undefined
    const fresh = snapshot([])
    const { loadProviderConfig, hostEpoch } = await setup(
      fresh,
      fresh,
      true,
      undefined,
      undefined,
      () => new Promise(resolve => (answer = resolve))
    )

    // The old host answers after the new one is on screen: System as it had it, added.
    hostEpoch.set('restarted')
    answer(snapshot([{ source: 'system', name: 'system' }]))

    await vi.waitFor(() => expect(loadProviderConfig).toHaveBeenCalledTimes(2))
    const system = await screen.findByRole('button', { name: 'System' })
    expect(system.textContent).not.toContain('Added · Open')
  })

  it('describes each source as the backend declares it', async () => {
    await setup(snapshot([]), snapshot([]))

    expect((await screen.findByRole('button', { name: 'GitLab' })).textContent).toContain(
      'What GitLab collects'
    )
  })

  it('opens the connection the backend reports as created', async () => {
    const work = { source: 'gitlab', name: 'work' } as const
    const created = { source: 'gitlab', name: 'gitlab' } as const
    const { opened } = await setup(snapshot([work]), snapshot([created, work]), true, created)

    await userEvent.click(await screen.findByRole('button', { name: 'GitLab' }))

    expect(opened).toHaveBeenCalledWith('gitlab/gitlab')
  })

  it('opens a single source that is already added, and disables unsupported ones', async () => {
    const { addConnection, opened } = await setup(
      snapshot([{ source: 'system', name: 'system' }]),
      snapshot([])
    )

    const system = await screen.findByRole('button', { name: 'System' })
    expect(system.getAttribute('aria-describedby')).toContain('tile-system-state')
    expect(system.textContent).toContain('Added · Open')
    await userEvent.click(system)
    expect(opened).toHaveBeenCalledWith('system/system')

    const outlook = screen.getByRole('button', { name: 'Outlook' })
    expect(outlook).toHaveProperty('disabled', true)
    expect(outlook.textContent).toContain('Windows and macOS only')
    expect(addConnection).not.toHaveBeenCalled()
  })

  it('shows a source another program added meanwhile as added, with why it was not', async () => {
    const reads = [snapshot([]), snapshot([{ source: 'system', name: 'system' }])]
    const loadProviderConfig = vi.fn(() => Promise.resolve(reads.shift() ?? snapshot([])))
    const refused = new BackendFailure('alreadyExists', 'System is already added')
    await render(SettingsAddSourcePaneComponent, {
      providers: [
        {
          provide: ConnectionSettings,
          useValue: { loadProviderConfig, addConnection: vi.fn().mockRejectedValue(refused) },
        },
        { provide: ActivityDataService, useValue: { hostEpoch: signal('mock-host') } },
        { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => true } },
      ],
    })

    await userEvent.click(await screen.findByRole('button', { name: 'System' }))

    await vi.waitFor(() =>
      expect(screen.getByRole('button', { name: 'System' }).textContent).toContain('Added · Open')
    )
    expect(loadProviderConfig).toHaveBeenCalledTimes(2)
    expect(screen.getByText('System is already added')).toBeTruthy()
  })

  it('reads the snapshot again only when another program got there first', async () => {
    const loadProviderConfig = vi.fn(() => Promise.resolve(snapshot([])))
    const refused = new BackendFailure('busy', 'The settings file is busy; try again.')
    await render(SettingsAddSourcePaneComponent, {
      providers: [
        {
          provide: ConnectionSettings,
          useValue: { loadProviderConfig, addConnection: vi.fn().mockRejectedValue(refused) },
        },
        { provide: ActivityDataService, useValue: { hostEpoch: signal('mock-host') } },
        { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => true } },
      ],
    })

    await userEvent.click(await screen.findByRole('button', { name: 'System' }))

    expect(await screen.findByText('The settings file is busy; try again.')).toBeTruthy()
    expect(loadProviderConfig).toHaveBeenCalledTimes(1)
  })

  it('counts the connections a kind already has', async () => {
    await setup(
      snapshot([
        { source: 'gitlab', name: 'work' },
        { source: 'gitlab', name: 'oss' },
      ]),
      snapshot([])
    )

    expect((await screen.findByRole('button', { name: 'GitLab' })).textContent).toContain(
      '2 added · Add another'
    )
  })

  it('asks Git where to look when WSL distributions are installed', async () => {
    const { addConnection, opened } = await setup(
      snapshot([], ['NixOS']),
      snapshot([{ source: 'git', name: 'wsl-nixos' }], ['NixOS'])
    )

    await userEvent.click(await screen.findByRole('button', { name: 'Git' }))
    expect(screen.getByRole('heading', { name: 'Where should Git look?' })).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'WSL · NixOS' }))

    expect(addConnection).toHaveBeenCalledWith({ source: 'git', wslDistro: 'NixOS' })
    expect(opened).toHaveBeenCalledWith('git/wsl-nixos')
  })

  it('adds Figma from its enabled tile in Add source', async () => {
    const { addConnection, opened } = await setup(
      snapshot([]),
      snapshot([{ source: 'figma', name: 'design' }])
    )

    const upcoming = await screen.findByRole('list', { name: 'Coming soon' })
    expect(within(upcoming).queryByText('Figma')).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Figma' }))

    expect(addConnection).toHaveBeenCalledWith({ source: 'figma' })
    expect(opened).toHaveBeenCalledWith('figma/design')
  })

  it('offers GitHub as an active source and shows upcoming sources as disabled', async () => {
    const { addConnection, opened } = await setup(
      snapshot([]),
      snapshot([{ source: 'github', name: 'github' }])
    )

    const tile = await screen.findByRole('button', { name: 'GitHub' })
    const upcoming = within(screen.getByRole('list', { name: 'Coming soon' }))
    expect(upcoming.queryByText('GitHub')).toBeNull()
    expect(upcoming.getByText('Microsoft Teams')).toBeTruthy()
    expect(upcoming.queryByRole('button')).toBeNull()

    await userEvent.click(tile)
    expect(addConnection).toHaveBeenCalledWith({ source: 'github' })
    expect(opened).toHaveBeenCalledWith('github/github')
  })

  it('points to the desktop app and the CLI in the browser', async () => {
    await setup(snapshot([]), snapshot([]), false)

    expect(screen.getByText(/activity-log sources add/)).toBeTruthy()
  })
})
