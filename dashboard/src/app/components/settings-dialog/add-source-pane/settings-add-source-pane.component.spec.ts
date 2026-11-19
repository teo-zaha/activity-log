import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type {
  ConnectionId,
  ProviderConfigSnapshotDto,
  ProviderKindDto,
} from '../../../generated/activity'
import { ActivityService } from '../../../services/activity.service'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
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
  kinds: [
    kind({ source: 'git', label: 'Git', wslDistros }),
    kind({ source: 'gitlab', label: 'GitLab' }),
    kind({ source: 'system', label: 'System', multiple: false }),
    kind({
      source: 'outlook',
      label: 'Outlook',
      multiple: false,
      supported: false,
      supportedOs: ['windows', 'macos'],
    }),
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
  created: ConnectionId | undefined = after.connections.at(-1)?.connection
): Promise<{ addConnection: ReturnType<typeof vi.fn>; opened: ReturnType<typeof vi.fn> }> {
  const addConnection = vi.fn().mockResolvedValue({
    created,
    snapshot: after,
    providersResponse: { currentOs: 'windows', providers: providerInfo },
  })
  const opened = vi.fn()
  await render(SettingsAddSourcePaneComponent, {
    on: { opened },
    providers: [
      {
        provide: ActivityService,
        useValue: {
          loadProviderConfig: (): Promise<ProviderConfigSnapshotDto> => Promise.resolve(before),
          addConnection,
        },
      },
      { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => desktop } },
    ],
  })
  return { addConnection, opened }
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

  it('shows upcoming sources as disabled', async () => {
    await setup(snapshot([]), snapshot([]))

    await screen.findByRole('button', { name: 'GitLab' })
    expect(screen.getByText('GitHub')).toBeTruthy()
    expect(screen.getByRole('list', { name: 'Coming soon' }).textContent).toContain('GitHub')
    expect(screen.queryByRole('button', { name: 'GitHub' })).toBeNull()
  })

  it('points to the desktop app and the CLI in the browser', async () => {
    await setup(snapshot([]), snapshot([]), false)

    expect(screen.getByText(/activity-log sources add/)).toBeTruthy()
  })
})
