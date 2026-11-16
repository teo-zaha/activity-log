import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type {
  ProviderConfigFieldStateDto,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
} from '../../../generated/activity'
import { ActivityService } from '../../../services/activity.service'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
import { providerInfo } from '../../../testing/activity-fixtures'
import type { SettingsProviderRow } from '../settings-dialog.types'
import { SettingsSourcePaneComponent } from './settings-source-pane.component'

const row = (index: 0 | 1): SettingsProviderRow => ({
  info: providerInfo[index],
  label: index === 0 ? 'Git' : 'Jira',
  icon: 'commit',
  tooltip: null,
})

const jiraFields: ProviderConfigFieldStateDto[] = [
  {
    key: 'baseUrl',
    label: 'Base URL',
    required: true,
    shape: 'single',
    valueType: 'url',
    secret: false,
    isSet: true,
    value: 'https://jira.example.com',
  },
  {
    key: 'maxResults',
    label: 'Max results',
    required: false,
    shape: 'single',
    valueType: 'int',
    secret: false,
    isSet: true,
    value: '50',
  },
  {
    key: 'apiToken',
    label: 'API token',
    required: true,
    shape: 'single',
    valueType: 'password',
    secret: true,
    isSet: true,
  },
  {
    key: 'verifyTls',
    label: 'Verify TLS',
    required: false,
    shape: 'single',
    valueType: 'bool',
    secret: false,
    isSet: true,
    value: 'true',
  },
]

const snapshot = (
  fields: ProviderConfigFieldStateDto[] = jiraFields
): ProviderConfigSnapshotDto => ({
  providers: [
    { source: 'git', fields: [] },
    { source: 'jira', fields },
  ],
})

interface Setup {
  switchProvider: (provider: SettingsProviderRow) => Promise<void>
  loadProviderConfig: ReturnType<typeof vi.fn<() => Promise<ProviderConfigSnapshotDto>>>
  saveProviderConfig: ReturnType<
    typeof vi.fn<(request: ProviderConfigSaveRequestDto) => Promise<ProviderConfigSaveResponseDto>>
  >
}

async function setup(
  options: {
    desktop?: boolean
    provider?: SettingsProviderRow
    load?: () => Promise<ProviderConfigSnapshotDto>
  } = {}
): Promise<Setup> {
  const loadProviderConfig = vi.fn<() => Promise<ProviderConfigSnapshotDto>>(
    options.load ?? ((): Promise<ProviderConfigSnapshotDto> => Promise.resolve(snapshot()))
  )
  const saveProviderConfig =
    vi.fn<(request: ProviderConfigSaveRequestDto) => Promise<ProviderConfigSaveResponseDto>>()
  const { rerender } = await render(SettingsSourcePaneComponent, {
    inputs: { provider: options.provider ?? row(1) },
    providers: [
      { provide: ActivityService, useValue: { loadProviderConfig, saveProviderConfig } },
      {
        provide: RuntimeEnvironmentService,
        useValue: { isTauri: (): boolean => options.desktop ?? true },
      },
    ],
  })
  return {
    loadProviderConfig,
    saveProviderConfig,
    switchProvider: provider => rerender({ partialUpdate: true, inputs: { provider } }),
  }
}

function saveResponse(fields: ProviderConfigFieldStateDto[]): ProviderConfigSaveResponseDto {
  return {
    snapshot: snapshot(fields),
    providersResponse: { currentOs: 'linux', providers: providerInfo },
  }
}

type ListField = Extract<ProviderConfigFieldStateDto, { shape: 'list' }>
type MapField = Extract<ProviderConfigFieldStateDto, { shape: 'map' }>

describe('SettingsSourcePaneComponent', () => {
  it('loads the configuration and fills the fields of the selected provider', async () => {
    const { loadProviderConfig } = await setup()

    expect(await screen.findByLabelText('Base URL')).toHaveProperty(
      'value',
      'https://jira.example.com'
    )
    expect(screen.getByLabelText('Max results')).toHaveProperty('value', '50')
    expect(screen.getByLabelText('API token')).toHaveProperty('value', '')
    expect(screen.getByText('Set')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
    expect(loadProviderConfig).toHaveBeenCalledTimes(1)
  })

  it('shows a provider without configurable keys', async () => {
    await setup({ provider: row(0) })

    expect(await screen.findByText(/does not expose configurable keys/)).toBeTruthy()
  })

  it('shows the load error and retries', async () => {
    const load = vi
      .fn<() => Promise<ProviderConfigSnapshotDto>>()
      .mockRejectedValueOnce(new Error('keychain locked'))
      .mockResolvedValue(snapshot())
    await setup({ load })

    expect((await screen.findByRole('alert')).textContent).toContain('keychain locked')

    await userEvent.click(screen.getByRole('button', { name: 'Retry' }))

    expect(await screen.findByLabelText('Base URL')).toBeTruthy()
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('flags invalid values and blocks saving until they are fixed', async () => {
    await setup()
    const baseUrl = await screen.findByLabelText('Base URL')

    await userEvent.clear(baseUrl)
    await userEvent.type(baseUrl, 'not a url')
    expect(await screen.findByText('Enter a valid URL.')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)

    await userEvent.clear(baseUrl)
    await userEvent.type(baseUrl, 'https://jira.internal')
    expect(screen.queryByText('Enter a valid URL.')).toBeNull()
    expect(screen.getByText('Unsaved changes')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', false)
  })

  it('saves only the changes and applies the returned snapshot', async () => {
    const { saveProviderConfig } = await setup()
    saveProviderConfig.mockResolvedValue(
      saveResponse(
        jiraFields.map(field => (field.key === 'maxResults' ? { ...field, value: '25' } : field))
      )
    )
    const maxResults = await screen.findByLabelText('Max results')

    await userEvent.clear(maxResults)
    await userEvent.type(maxResults, '25')
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

    expect(saveProviderConfig).toHaveBeenCalledWith({
      source: 'jira',
      values: [{ key: 'maxResults', value: '25' }],
    })
    expect(await screen.findByText('Settings saved.')).toBeTruthy()
    expect(screen.queryByText('Unsaved changes')).toBeNull()
    expect(screen.getByLabelText('Max results')).toHaveProperty('value', '25')
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
  })

  it('sends a new secret and clears a stored one', async () => {
    const { saveProviderConfig } = await setup()
    saveProviderConfig.mockResolvedValue(saveResponse(jiraFields))
    const token = await screen.findByLabelText('API token')

    await userEvent.type(token, 's3cret')
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
    expect(saveProviderConfig).toHaveBeenLastCalledWith({
      source: 'jira',
      values: [{ key: 'apiToken', value: 's3cret' }],
    })
    await screen.findByText('Settings saved.')

    await userEvent.click(screen.getByRole('button', { name: 'Clear' }))
    expect(screen.getByText('Not set')).toBeTruthy()
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
    expect(saveProviderConfig).toHaveBeenLastCalledWith({
      source: 'jira',
      values: [{ key: 'apiToken', value: null }],
    })
  })

  it('edits booleans through the select', async () => {
    const { saveProviderConfig } = await setup()
    saveProviderConfig.mockResolvedValue(saveResponse(jiraFields))

    await userEvent.click(await screen.findByRole('combobox'))
    await userEvent.click(await screen.findByRole('option', { name: 'False' }))
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

    expect(saveProviderConfig).toHaveBeenCalledWith({
      source: 'jira',
      values: [{ key: 'verifyTls', value: 'false' }],
    })
  })

  it('shows a save failure', async () => {
    const { saveProviderConfig } = await setup()
    saveProviderConfig.mockRejectedValue('write refused')
    const maxResults = await screen.findByLabelText('Max results')

    await userEvent.clear(maxResults)
    await userEvent.type(maxResults, '25')
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

    expect((await screen.findByRole('alert')).textContent).toContain('write refused')
    expect(screen.queryByText('Settings saved.')).toBeNull()
  })

  it('does not load anything outside the desktop app', async () => {
    const { loadProviderConfig } = await setup({ desktop: false })

    expect(screen.getByText(/editable only in the desktop app/)).toBeTruthy()
    expect(screen.queryByLabelText('Base URL')).toBeNull()
    expect(loadProviderConfig).not.toHaveBeenCalled()
  })

  it('explains a number input that holds something unparseable', async () => {
    await setup()
    const maxResults = await screen.findByLabelText('Max results')
    Object.defineProperty(maxResults, 'validity', {
      value: { badInput: true },
      configurable: true,
    })

    await userEvent.type(maxResults, '-')

    expect(await screen.findByText('Enter a whole number.')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
  })

  it('masks every secret, so clearing one and typing again stays a plain string', async () => {
    const { saveProviderConfig } = await setup({
      load: () =>
        Promise.resolve(
          snapshot([
            {
              key: 'webhook',
              label: 'Webhook',
              required: false,
              shape: 'single',
              valueType: 'string',
              secret: true,
              isSet: true,
            },
          ])
        ),
    })
    saveProviderConfig.mockResolvedValue(saveResponse(jiraFields))
    const webhook = await screen.findByLabelText('Webhook')
    expect(webhook).toHaveProperty('type', 'password')

    await userEvent.click(screen.getByRole('button', { name: 'Clear' }))
    await userEvent.type(webhook, 'abc')
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

    expect(saveProviderConfig).toHaveBeenCalledWith({
      source: 'jira',
      values: [{ key: 'webhook', value: 'abc' }],
    })
  })

  it('starts every source from its own saved values, without leftovers', async () => {
    const gitFields: ProviderConfigFieldStateDto[] = [
      {
        key: 'gitRoot',
        label: 'Git root',
        required: false,
        shape: 'single',
        valueType: 'string',
        secret: false,
        isSet: true,
        value: '/code',
      },
    ]
    const both = (): Promise<ProviderConfigSnapshotDto> =>
      Promise.resolve({
        providers: [
          { source: 'git', fields: gitFields },
          { source: 'jira', fields: jiraFields },
        ],
      })
    const { saveProviderConfig, switchProvider } = await setup({ load: both })
    saveProviderConfig.mockRejectedValue(new Error('write refused'))

    const baseUrl = await screen.findByLabelText('Base URL')
    await userEvent.clear(baseUrl)
    await userEvent.type(baseUrl, 'https://jira.internal')
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
    expect((await screen.findByRole('alert')).textContent).toContain('write refused')

    await switchProvider(row(0))

    expect(await screen.findByLabelText('Git root')).toHaveProperty('value', '/code')
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByText('Unsaved changes')).toBeNull()
    expect(screen.queryByLabelText('Base URL')).toBeNull()

    await switchProvider(row(1))

    expect(await screen.findByLabelText('Base URL')).toHaveProperty(
      'value',
      'https://jira.example.com'
    )
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.queryByText('Unsaved changes')).toBeNull()
  })

  describe('list fields', () => {
    const gitDirs = (
      overrides: Partial<Omit<ListField, 'shape'>> = {}
    ): ProviderConfigFieldStateDto => ({
      key: 'GIT_DIRS',
      label: 'Git Dirs',
      required: false,
      isSet: false,
      shape: 'list',
      itemType: 'string',
      items: ['/home/me'],
      ...overrides,
    })

    async function setupList(field: ProviderConfigFieldStateDto): Promise<Setup> {
      const result = await setup({ load: () => Promise.resolve(snapshot([field])) })
      result.saveProviderConfig.mockResolvedValue(saveResponse([field]))
      await screen.findByLabelText('Git Dirs 1')
      return result
    }

    it('shows the default items and treats them as unchanged', async () => {
      await setupList(gitDirs())

      expect(screen.getByLabelText('Git Dirs 1')).toHaveProperty('value', '/home/me')
      expect(screen.getByText('Default')).toBeTruthy()
      expect(screen.queryByRole('button', { name: 'Use default' })).toBeNull()
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
    })

    it('adds rows and saves the list without blank ones', async () => {
      const { saveProviderConfig } = await setupList(gitDirs())

      await userEvent.click(screen.getByRole('button', { name: 'Add' }))
      await userEvent.type(screen.getByLabelText('Git Dirs 2'), ' /srv/repos ')
      await userEvent.click(screen.getByRole('button', { name: 'Add' }))
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenCalledWith({
        source: 'jira',
        values: [{ key: 'GIT_DIRS', value: ['/home/me', '/srv/repos'] }],
      })
    })

    it('saves an emptied list as none', async () => {
      const { saveProviderConfig } = await setupList(gitDirs())

      await userEvent.click(screen.getByRole('button', { name: 'Remove /home/me' }))
      expect(screen.getByText('None')).toBeTruthy()
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenCalledWith({
        source: 'jira',
        values: [{ key: 'GIT_DIRS', value: [] }],
      })
    })

    it('checks every item against the value type', async () => {
      await setupList(gitDirs({ itemType: 'url', items: ['https://a.example'] }))

      await userEvent.click(screen.getByRole('button', { name: 'Add' }))
      await userEvent.type(screen.getByLabelText('Git Dirs 2'), 'not a url')

      expect(await screen.findByText('Enter a valid URL.')).toBeTruthy()
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
    })

    it('resets a configured list to its default', async () => {
      const { saveProviderConfig } = await setupList(gitDirs({ isSet: true, items: ['/work'] }))

      await userEvent.click(screen.getByRole('button', { name: 'Use default' }))
      expect(screen.getByText('The default applies after saving.')).toBeTruthy()
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenCalledWith({
        source: 'jira',
        values: [{ key: 'GIT_DIRS', value: null }],
      })
    })
  })

  describe('map fields', () => {
    const wslDirs = (
      overrides: Partial<Omit<MapField, 'shape'>> = {}
    ): ProviderConfigFieldStateDto => ({
      key: 'WSL_GIT_DIRS',
      label: 'WSL Git Dirs',
      required: false,
      isSet: false,
      shape: 'map',
      itemType: 'string',
      entries: [{ name: 'NixOS', items: ['~'] }],
      names: ['NixOS', 'Ubuntu'],
      ...overrides,
    })

    async function setupMap(field: ProviderConfigFieldStateDto): Promise<Setup> {
      const result = await setup({ load: () => Promise.resolve(snapshot([field])) })
      result.saveProviderConfig.mockResolvedValue(saveResponse([field]))
      await screen.findByLabelText('WSL Git Dirs 1')
      return result
    }

    it('shows the default entries with their items, unchanged', async () => {
      await setupMap(wslDirs())

      expect(screen.getByLabelText('NixOS 1')).toHaveProperty('value', '~')
      expect(screen.getByText('Default')).toBeTruthy()
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
    })

    it('adds folders under a distribution and saves lists by name', async () => {
      const { saveProviderConfig } = await setupMap(wslDirs())

      await userEvent.click(screen.getByRole('button', { name: 'Add to NixOS' }))
      await userEvent.type(screen.getByLabelText('NixOS 2'), ' /srv/tool ')
      await userEvent.click(screen.getByRole('button', { name: 'Add Ubuntu' }))
      await userEvent.type(screen.getByLabelText('Ubuntu 1'), '~/work')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenCalledWith({
        source: 'jira',
        values: [{ key: 'WSL_GIT_DIRS', value: { NixOS: ['~', '/srv/tool'], Ubuntu: ['~/work'] } }],
      })
    })

    it('offers each name once and stops adding when all are used', async () => {
      await setupMap(wslDirs())

      await userEvent.click(screen.getByRole('button', { name: 'Add Ubuntu' }))

      expect(screen.queryByRole('button', { name: /^Add (NixOS|Ubuntu)$/ })).toBeNull()
      await userEvent.click(screen.getByLabelText('WSL Git Dirs 2'))
      const options = await screen.findAllByRole('option')
      expect(options.map(option => option.textContent?.trim())).toEqual(['Ubuntu'])
    })

    it('saves an emptied map as none', async () => {
      const { saveProviderConfig } = await setupMap(wslDirs())

      await userEvent.click(screen.getByRole('button', { name: 'Remove NixOS' }))
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenCalledWith({
        source: 'jira',
        values: [{ key: 'WSL_GIT_DIRS', value: {} }],
      })
    })

    it('resets a configured map to its default', async () => {
      const { saveProviderConfig } = await setupMap(
        wslDirs({ isSet: true, entries: [{ name: 'NixOS', items: ['/work'] }] })
      )

      await userEvent.click(screen.getByRole('button', { name: 'Use default' }))
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenCalledWith({
        source: 'jira',
        values: [{ key: 'WSL_GIT_DIRS', value: null }],
      })
    })
  })
})
