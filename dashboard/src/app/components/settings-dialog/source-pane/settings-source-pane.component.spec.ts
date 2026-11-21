import { signal, type WritableSignal } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { render, screen, within } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type {
  AddConnectionRequestDto,
  AddConnectionResponseDto,
  ProviderConfigFieldIssueDto,
  RemoveConnectionRequestDto,
  ConnectionId,
  InstallReceipt,
  ProviderConfigConnectionStateDto,
  ProviderConfigFieldStateDto,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
} from '../../../generated/activity'
import { ActivityDataService } from '../../../services/activity-data.service'
import { ConnectionSettings } from '../../../services/connection-settings.service'
import type { ReceiptStanding } from '../../../services/types'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
import { providerInfo } from '../../../testing/activity-fixtures'
import type { SettingsProviderRow } from '../settings-dialog.types'
import { SettingsSourcePaneComponent } from './settings-source-pane.component'
import { SourceDrafts } from './source-drafts.service'
import { ToastService } from '../../../services/toast.service'
import { BackendFailure } from '../../../services/backend/backend-failure'

const row = (index: 0 | 1): SettingsProviderRow => ({
  info: providerInfo[index],
  key: index === 0 ? 'git/default' : 'jira/default',
  label: index === 0 ? 'Git' : 'Jira',
  kindLabel: index === 0 ? 'Git' : 'Jira',
  tooltip: null,
})

const state = (
  source: 'git' | 'jira',
  fields: ProviderConfigFieldStateDto[]
): ProviderConfigConnectionStateDto => ({
  connection: { source, name: 'default' },
  label: source === 'git' ? 'This computer' : 'Jira',
  fields,
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
  revision: 'r1',
  kinds: [],
  connections: [state('git', []), state('jira', fields)],
})

type CheckProviderConfig = (
  request: ProviderConfigSaveRequestDto
) => Promise<ProviderConfigFieldIssueDto[]>

interface Setup {
  switchProvider: (provider: SettingsProviderRow) => Promise<void>
  /** The backend's check of a draft; no issues unless a test says otherwise. */
  checkProviderConfig: ReturnType<typeof vi.fn<CheckProviderConfig>>
  /** Resolves once the pane has handled what is pending and rendered it. */
  whenStable: () => Promise<void>
  loadProviderConfig: ReturnType<typeof vi.fn<() => Promise<ProviderConfigSnapshotDto>>>
  saveProviderConfig: ReturnType<
    typeof vi.fn<(request: ProviderConfigSaveRequestDto) => Promise<ProviderConfigSaveResponseDto>>
  >
  recheckConnection: ReturnType<typeof vi.fn<(connection: ConnectionId) => Promise<void>>>
  /** Where the coordinator puts a save's receipt; `current` unless a test says otherwise. */
  receiptStanding: ReturnType<typeof vi.fn<(receipt: InstallReceipt) => ReceiptStanding>>
  /** The host on screen, as the coordinator has it. */
  hostEpoch: WritableSignal<string | null>
  removeConnection: ReturnType<
    typeof vi.fn<(request: RemoveConnectionRequestDto) => Promise<ProviderConfigSaveResponseDto>>
  >
  addConnection: ReturnType<
    typeof vi.fn<(request: AddConnectionRequestDto) => Promise<AddConnectionResponseDto>>
  >
  /** The pane's `removed` output. */
  removed: ReturnType<typeof vi.fn>
  /** The pane's `opened` output: a connection to show instead. */
  opened: ReturnType<typeof vi.fn>
}

async function setup(
  options: {
    desktop?: boolean
    provider?: SettingsProviderRow
    load?: () => Promise<ProviderConfigSnapshotDto>
    drafts?: SourceDrafts
  } = {}
): Promise<Setup> {
  const loadProviderConfig = vi.fn<() => Promise<ProviderConfigSnapshotDto>>(
    options.load ?? ((): Promise<ProviderConfigSnapshotDto> => Promise.resolve(snapshot()))
  )
  const saveProviderConfig =
    vi.fn<(request: ProviderConfigSaveRequestDto) => Promise<ProviderConfigSaveResponseDto>>()
  const recheckConnection = vi.fn<(connection: ConnectionId) => Promise<void>>(() =>
    Promise.resolve()
  )
  const receiptStanding = vi.fn<(receipt: InstallReceipt) => ReceiptStanding>(() => 'current')
  const checkProviderConfig = vi.fn<CheckProviderConfig>(() => Promise.resolve([]))
  const hostEpoch = signal<string | null>('mock-host')
  const removeConnection =
    vi.fn<(request: RemoveConnectionRequestDto) => Promise<ProviderConfigSaveResponseDto>>()
  const addConnection =
    vi.fn<(request: AddConnectionRequestDto) => Promise<AddConnectionResponseDto>>()
  const removed = vi.fn()
  const opened = vi.fn()
  const { rerender, fixture } = await render(SettingsSourcePaneComponent, {
    inputs: { provider: options.provider ?? row(1) },
    on: { removed, opened },
    providers: [
      {
        provide: ConnectionSettings,
        useValue: {
          loadProviderConfig,
          saveProviderConfig,
          checkProviderConfig,
          removeConnection,
          addConnection,
        },
      },
      {
        provide: ActivityDataService,
        useValue: {
          recheckConnection,
          receiptStanding,
          hostEpoch,
        },
      },
      {
        provide: RuntimeEnvironmentService,
        useValue: { isTauri: (): boolean => options.desktop ?? true },
      },
      ...(options.drafts ? [{ provide: SourceDrafts, useValue: options.drafts }] : []),
    ],
  })
  return {
    removeConnection,
    addConnection,
    removed,
    opened,
    loadProviderConfig,
    saveProviderConfig,
    checkProviderConfig,
    recheckConnection,
    receiptStanding,
    hostEpoch,
    switchProvider: provider => rerender({ partialUpdate: true, inputs: { provider } }),
    whenStable: () => fixture.whenStable(),
  }
}

/** The next confirmation the pane shows. */
function nextToast(): Promise<string> {
  const toast = TestBed.inject(ToastService)
  return new Promise(resolve => {
    const success = vi.spyOn(toast, 'success').mockImplementationOnce(message => {
      success.mockRestore()
      resolve(message)
    })
  })
}

function saveResponse(fields: ProviderConfigFieldStateDto[]): ProviderConfigSaveResponseDto {
  return {
    snapshot: snapshot(fields),
    providersResponse: { currentOs: 'linux', providers: providerInfo },
    installed: { epoch: 'mock-host', revision: 1 },
  }
}

type ListField = Extract<ProviderConfigFieldStateDto, { shape: 'list' }>

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

  it('titles the health card by the problem the startup check reported', async () => {
    await setup({
      provider: {
        ...row(1),
        info: {
          ...providerInfo[1],
          status: 'unavailable',
          usable: false,
          reason: 'Startup check timed out after 15s',
          problem: 'timedOut',
          recheckable: true,
        },
      },
    })

    expect(await screen.findByRole('heading', { name: 'Startup check timed out' })).toBeTruthy()
    expect(screen.getByText('timer_off')).toBeTruthy()
  })

  describe('operations and the snapshot', () => {
    const degraded = (): SettingsProviderRow => ({
      ...row(1),
      info: {
        ...providerInfo[1],
        status: 'degraded',
        reason: 'Collection failed',
        problem: 'collectionFailed',
        recheckable: true,
      },
    })

    it('keeps a snapshot read after the save was sent over the save’s own', async () => {
      let disk = snapshot()
      const { saveProviderConfig, switchProvider, whenStable } = await setup({
        load: () => Promise.resolve(disk),
      })
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      saveProviderConfig.mockImplementationOnce(() => new Promise(resolve => (answer = resolve)))
      const max = await screen.findByLabelText('Max results')
      await userEvent.clear(max)
      await userEvent.type(max, '40')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
      // Another writer clears the token; a read shows it before the sources on screen do.
      disk = snapshot(jiraFields.map(f => (f.key === 'apiToken' ? { ...f, isSet: false } : f)))
      await switchProvider(row(0))
      await switchProvider(row(1))
      await whenStable()
      expect(screen.queryByRole('button', { name: 'Clear saved API token' })).toBeNull()

      const toasted = nextToast()
      answer(saveResponse(jiraFields))
      await toasted
      await whenStable()

      expect(screen.queryByRole('button', { name: 'Clear saved API token' })).toBeNull()
    })

    it('reads the snapshot again when a read fails after the host changed', async () => {
      const { saveProviderConfig, hostEpoch, receiptStanding, loadProviderConfig, whenStable } =
        await setup()
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      let fail = (_: Error): void => undefined
      saveProviderConfig.mockImplementationOnce(() => new Promise(resolve => (answer = resolve)))
      const max = await screen.findByLabelText('Max results')
      await userEvent.clear(max)
      await userEvent.type(max, '40')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
      hostEpoch.set('b')
      receiptStanding.mockReturnValue('otherHost')
      loadProviderConfig.mockImplementationOnce(() => new Promise((_, reject) => (fail = reject)))
      const toasted = nextToast()
      answer(saveResponse(jiraFields))
      await toasted
      await vi.waitFor(() => expect(loadProviderConfig).toHaveBeenCalledTimes(2))

      // Host b goes away with its read; host c is read instead, and its snapshot shows.
      hostEpoch.set('c')
      fail(new BackendFailure('internal', 'Host B exited'))
      await whenStable()

      expect(loadProviderConfig).toHaveBeenCalledTimes(3)
      expect(screen.queryByRole('alert')).toBeNull()
    })

    it('keeps the form busy while its save is out, also after a check ends', async () => {
      const { saveProviderConfig, whenStable } = await setup({ provider: degraded() })
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      saveProviderConfig.mockImplementationOnce(() => new Promise(resolve => (answer = resolve)))
      await userEvent.type(await screen.findByLabelText('Base URL'), '/other')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      await userEvent.click(screen.getByRole('button', { name: 'Check again' }))
      await whenStable()

      // No second save can start before the first is answered, so none can overtake it.
      expect(screen.getByLabelText('Base URL')).toHaveProperty('disabled', true)
      expect(screen.getByRole('button', { name: 'Saving…' })).toHaveProperty('disabled', true)
      const toasted = nextToast()
      answer(saveResponse(jiraFields))
      await toasted
      await whenStable()
      expect(screen.getByLabelText('Base URL')).toHaveProperty('disabled', false)
    })

    it('reads the snapshot again when a later install than the save’s is on screen', async () => {
      let current = snapshot()
      const { saveProviderConfig, whenStable, loadProviderConfig, receiptStanding } = await setup({
        load: () => Promise.resolve(current),
      })
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      saveProviderConfig.mockImplementationOnce(() => new Promise(resolve => (answer = resolve)))
      const max = await screen.findByLabelText('Max results')
      await userEvent.clear(max)
      await userEvent.type(max, '40')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
      // The token is cleared elsewhere, and that install is on screen before the save's answer.
      current = snapshot(jiraFields.map(f => (f.key === 'apiToken' ? { ...f, isSet: false } : f)))
      receiptStanding.mockReturnValue('superseded')
      const reads = loadProviderConfig.mock.calls.length

      const toasted = nextToast()
      answer(
        saveResponse(jiraFields.map(f => (f.key === 'maxResults' ? { ...f, value: '40' } : f)))
      )
      await toasted
      await whenStable()

      expect(receiptStanding).toHaveBeenCalledWith({ epoch: 'mock-host', revision: 1 })
      expect(loadProviderConfig.mock.calls.length).toBe(reads + 1)
      expect(screen.queryByRole('button', { name: 'Clear saved API token' })).toBeNull()
    })

    it('reads the snapshot again when the host changed while it was read', async () => {
      const { saveProviderConfig, loadProviderConfig, whenStable, receiptStanding, hostEpoch } =
        await setup()
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      saveProviderConfig.mockImplementationOnce(() => new Promise(resolve => (answer = resolve)))
      const max = await screen.findByLabelText('Max results')
      await userEvent.clear(max)
      await userEvent.type(max, '40')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
      // Host a answers the save after host b replaced it: the snapshot is read under b...
      hostEpoch.set('b')
      receiptStanding.mockReturnValue('otherHost')
      let read = (_: ProviderConfigSnapshotDto): void => undefined
      loadProviderConfig.mockImplementationOnce(() => new Promise(resolve => (read = resolve)))
      const toasted = nextToast()
      answer(saveResponse(jiraFields))
      await toasted
      await vi.waitFor(() => expect(loadProviderConfig).toHaveBeenCalledTimes(2))

      // ...and comes back under c, which shows the token cleared.
      hostEpoch.set('c')
      loadProviderConfig.mockResolvedValue(
        snapshot(jiraFields.map(f => (f.key === 'apiToken' ? { ...f, isSet: false } : f)))
      )
      read(snapshot())
      await whenStable()

      expect(loadProviderConfig).toHaveBeenCalledTimes(3)
      expect(screen.queryByRole('button', { name: 'Clear saved API token' })).toBeNull()
    })

    it('sends only the field the user changed after the settings were read again', async () => {
      let current = snapshot()
      const { saveProviderConfig, whenStable, switchProvider } = await setup({
        load: () => Promise.resolve(current),
      })
      const max = await screen.findByLabelText('Max results')
      await userEvent.clear(max)
      await userEvent.type(max, '40')
      await switchProvider(row(0))
      await whenStable()
      // Another writer changed the URL.
      current = snapshot(
        jiraFields.map(f =>
          f.key === 'baseUrl' ? { ...f, value: 'https://newer.example.org' } : f
        )
      )
      await switchProvider(row(1))
      await whenStable()
      expect(screen.getByLabelText('Base URL')).toHaveProperty('value', 'https://newer.example.org')

      saveProviderConfig.mockResolvedValueOnce(saveResponse(jiraFields))
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig.mock.calls[0]?.[0].values).toEqual([
        { key: 'maxResults', value: '40' },
      ])
    })

    it('keeps a check’s failure with its source while another is shown', async () => {
      const { recheckConnection, switchProvider, whenStable } = await setup({
        provider: degraded(),
      })
      recheckConnection.mockRejectedValueOnce(
        new BackendFailure('notFound', 'No source jira/default')
      )
      await userEvent.click(await screen.findByRole('button', { name: 'Check again' }))
      await whenStable()
      expect(screen.getByRole('alert').textContent).toContain('No source jira/default')

      await switchProvider(row(0))
      await whenStable()
      expect(screen.queryByRole('alert')).toBeNull()
      await switchProvider(degraded())
      await whenStable()
      expect(screen.getByRole('alert').textContent).toContain('No source jira/default')
    })

    it('keeps a check’s failure that arrives while another source is shown', async () => {
      const { recheckConnection, switchProvider, whenStable } = await setup({
        provider: degraded(),
      })
      let refuse = (_: Error): void => undefined
      recheckConnection.mockImplementationOnce(() => new Promise((_, reject) => (refuse = reject)))
      await userEvent.click(await screen.findByRole('button', { name: 'Check again' }))
      await switchProvider(row(0))
      await whenStable()

      refuse(new BackendFailure('notFound', 'No source jira/default'))
      await whenStable()
      expect(screen.queryByRole('alert')).toBeNull()
      await switchProvider(degraded())
      await whenStable()
      expect(screen.getByRole('alert').textContent).toContain('No source jira/default')
    })
  })

  it('checks a failed source again from its health card', async () => {
    const failed = {
      ...providerInfo[1],
      status: 'unavailable',
      usable: false,
      reason: 'Startup check timed out after 15s',
      problem: 'timedOut',
      recheckable: true,
    } as const
    const { recheckConnection, switchProvider } = await setup({
      provider: { ...row(1), info: failed },
    })

    await userEvent.click(await screen.findByRole('button', { name: 'Check again' }))
    expect(recheckConnection).toHaveBeenCalledWith(failed.connection)

    // Nothing to check again on a source that works or cannot run on this OS.
    await switchProvider(row(1))
    expect(screen.queryByRole('button', { name: 'Check again' })).toBeNull()
    await switchProvider({
      ...row(1),
      info: { ...failed, problem: 'unsupported', reason: 'OS not supported', recheckable: false },
    })
    expect(screen.queryByRole('button', { name: 'Check again' })).toBeNull()
  })

  it('does not guess a timeout from the wording of a reason', async () => {
    await setup({
      provider: {
        ...row(1),
        info: {
          ...providerInfo[1],
          status: 'unavailable',
          usable: false,
          reason: 'The server timed out the token',
          problem: 'failed',
          recheckable: true,
        },
      },
    })

    expect(await screen.findByRole('heading', { name: 'Not available' })).toBeTruthy()
  })

  it('names a refused or unreachable collection by its problem, not as any failure', async () => {
    const collected = (problem: 'refused' | 'unreachable'): SettingsProviderRow => ({
      ...row(1),
      info: {
        ...providerInfo[1],
        status: 'degraded',
        reason: 'Collection failed: Jira answered 401 Unauthorized',
        problem,
        recheckable: true,
      },
    })
    const { switchProvider } = await setup({ provider: collected('refused') })

    expect(await screen.findByRole('heading', { name: 'Access refused' })).toBeTruthy()
    await switchProvider(collected('unreachable'))
    expect(await screen.findByRole('heading', { name: 'Server unreachable' })).toBeTruthy()
  })

  it.each([
    ['notInstalled', 'Not installed'],
    ['notRunning', 'Not running'],
    ['permissionDenied', 'Permission needed'],
    ['noData', 'Nothing to read'],
  ] as const)('titles a source whose check found %s', async (problem, title) => {
    await setup({
      provider: {
        ...row(1),
        info: {
          ...providerInfo[1],
          status: 'unavailable',
          usable: false,
          reason: 'From the check',
          problem,
          recheckable: true,
        },
      },
    })

    expect(await screen.findByRole('heading', { name: title })).toBeTruthy()
  })

  it('offers the label and removal for a source without settings of its own', async () => {
    await setup({ provider: row(0) })

    expect(await screen.findByText('Git needs no further settings.')).toBeTruthy()
    expect(screen.getByLabelText('Label')).toHaveProperty('value', 'This computer')
    expect(screen.getByRole('button', { name: 'Remove source' })).toBeTruthy()
  })

  it('shows the load error and retries', async () => {
    const load = vi
      .fn<() => Promise<ProviderConfigSnapshotDto>>()
      .mockRejectedValueOnce(new BackendFailure('internal', 'keychain locked'))
      .mockResolvedValue(snapshot())
    await setup({ load })

    expect((await screen.findByRole('alert')).textContent).toContain('keychain locked')

    await userEvent.click(screen.getByRole('button', { name: 'Retry' }))

    expect(await screen.findByLabelText('Base URL')).toBeTruthy()
    expect(load).toHaveBeenCalledTimes(2)
  })

  it('shows what the backend refuses as a field is left, and blocks saving until it changes', async () => {
    const { checkProviderConfig } = await setup()
    checkProviderConfig.mockResolvedValue([
      {
        field: { kind: 'value', key: 'baseUrl' },
        problem: 'invalid',
        message: 'Enter an http or https address.',
      },
    ])
    const baseUrl = await screen.findByLabelText('Base URL')

    await userEvent.clear(baseUrl)
    await userEvent.type(baseUrl, 'ftp://jira.internal')
    expect(screen.queryByText('Enter an http or https address.')).toBeNull()
    await userEvent.tab()
    expect(await screen.findByText('Enter an http or https address.')).toBeTruthy()
    expect(checkProviderConfig).toHaveBeenLastCalledWith({
      connection: { source: 'jira', name: 'default' },
      values: [{ key: 'baseUrl', value: 'ftp://jira.internal' }],
      basedOn: 'r1',
    })
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)

    // The issue was about the value it checked: another value shows none until it is checked.
    await userEvent.clear(baseUrl)
    await userEvent.type(baseUrl, 'https://jira.internal')
    expect(screen.queryByText('Enter an http or https address.')).toBeNull()
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', false)
  })

  it('drops a check that comes back after the draft it checked was changed', async () => {
    const { checkProviderConfig, whenStable } = await setup()
    let resolve: (issues: ProviderConfigFieldIssueDto[]) => void = () => undefined
    const answer = new Promise<ProviderConfigFieldIssueDto[]>(done => (resolve = done))
    checkProviderConfig.mockReturnValueOnce(answer)
    const baseUrl = await screen.findByLabelText('Base URL')

    await userEvent.clear(baseUrl)
    await userEvent.type(baseUrl, 'ftp://a')
    await userEvent.tab()
    await userEvent.type(baseUrl, '-b')
    resolve([
      {
        field: { kind: 'value', key: 'baseUrl' },
        problem: 'invalid',
        message: 'Enter an http or https address.',
      },
    ])
    await answer
    await whenStable()

    expect(screen.queryByText('Enter an http or https address.')).toBeNull()
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', false)
  })

  it('keeps a failed check with the draft it was sent for', async () => {
    const { checkProviderConfig, whenStable } = await setup()
    let reject: (error: unknown) => void = () => undefined
    const answer = new Promise<ProviderConfigFieldIssueDto[]>((_, fail) => (reject = fail))
    checkProviderConfig.mockReturnValueOnce(answer)
    const baseUrl = await screen.findByLabelText('Base URL')

    await userEvent.clear(baseUrl)
    await userEvent.type(baseUrl, 'https://a.example.org')
    await userEvent.tab()
    await userEvent.type(baseUrl, '/b')
    reject(new BackendFailure('internal', 'the check broke'))
    await answer.catch(() => undefined)
    await whenStable()
    expect(screen.queryByText(/Checking the settings failed|the check broke/)).toBeNull()

    // The same failure for the draft on screen shows.
    checkProviderConfig.mockRejectedValueOnce(new BackendFailure('internal', 'the check broke'))
    await userEvent.click(baseUrl)
    await userEvent.tab()
    await whenStable()
    expect(await screen.findByText(/the check broke/)).toBeTruthy()
  })

  describe('a save another program got in before', () => {
    /** The settings as another program left them: the Base URL changed, as `r2`. */
    const theirs = (): ProviderConfigSnapshotDto => ({
      ...snapshot(
        jiraFields.map(field =>
          field.key === 'baseUrl' ? { ...field, value: 'https://theirs.example.org' } : field
        )
      ),
      revision: 'r2',
    })
    const changedHere = (): ProviderConfigFieldIssueDto[] => [
      {
        field: { kind: 'value', key: 'baseUrl' },
        problem: 'changed',
        message: 'Another program changed this since you started editing.',
      },
    ]

    /** A first read as the pane opened, and every later one as another program left it. */
    const loads = (
      later: ProviderConfigSnapshotDto
    ): ReturnType<typeof vi.fn<() => Promise<ProviderConfigSnapshotDto>>> =>
      vi
        .fn<() => Promise<ProviderConfigSnapshotDto>>()
        .mockResolvedValueOnce(snapshot())
        .mockResolvedValue(later)

    it('reads the settings again when a check finds a conflict, and saves over the ones on screen', async () => {
      // Astra's probe: the Base URL changes on disk, the user edits it and leaves the field.
      const load = loads(theirs())
      const { saveProviderConfig, checkProviderConfig, loadProviderConfig, whenStable } =
        await setup({ load })
      checkProviderConfig.mockImplementation(request =>
        Promise.resolve(request.basedOn === 'r1' ? changedHere() : [])
      )
      const baseUrl = await screen.findByLabelText('Base URL')
      await userEvent.clear(baseUrl)
      await userEvent.type(baseUrl, 'https://mine.example.org')
      await userEvent.tab()
      await whenStable()

      expect(loadProviderConfig).toHaveBeenCalledTimes(2)
      expect(await screen.findByText(/Now: https:\/\/theirs\.example\.org/)).toBeTruthy()
      expect(baseUrl).toHaveProperty('value', 'https://mine.example.org')
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)

      await userEvent.click(screen.getByRole('button', { name: 'Keep my changes' }))
      await whenStable()
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty(
        'disabled',
        false
      )
      saveProviderConfig.mockResolvedValue(saveResponse(jiraFields))
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenLastCalledWith(
        expect.objectContaining({ basedOn: 'r2' })
      )
    })

    it('says what the label is now when another program changed it', async () => {
      const relabelled = { ...snapshot(), revision: 'r2' }
      relabelled.connections = relabelled.connections.map(state =>
        state.connection.source === 'jira' ? { ...state, label: 'Theirs' } : state
      )
      const { checkProviderConfig, whenStable } = await setup({ load: loads(relabelled) })
      checkProviderConfig.mockImplementation(request =>
        Promise.resolve(
          request.basedOn === 'r1'
            ? [
                {
                  field: { kind: 'label' },
                  problem: 'changed',
                  message: 'Another program changed this since you started editing.',
                },
              ]
            : []
        )
      )
      const label = await screen.findByLabelText('Label')
      await userEvent.clear(label)
      await userEvent.type(label, 'Mine')
      await userEvent.tab()
      await whenStable()

      expect(await screen.findByText(/Now: Theirs\./)).toBeTruthy()
    })

    it('moves the draft of a removed source to the connection added again, by its new id', async () => {
      // Astra's probe: another program removed `jira/default`, then restored that name; adding
      // Jira again created `jira/jira`. The draft goes with the source the user added.
      const gone: ProviderConfigSnapshotDto = { revision: 'r2', kinds: [], connections: [] }
      const drafts = new SourceDrafts()
      const { checkProviderConfig, addConnection, removed, opened, whenStable } = await setup({
        load: loads(gone),
        drafts,
      })
      checkProviderConfig.mockImplementation(request =>
        Promise.resolve(
          request.basedOn === 'r1'
            ? [
                {
                  field: { kind: 'connection' },
                  problem: 'removed',
                  message: 'Another program removed this source.',
                },
              ]
            : []
        )
      )
      const baseUrl = await screen.findByLabelText('Base URL')
      await userEvent.clear(baseUrl)
      await userEvent.type(baseUrl, 'https://mine.example.org')
      await userEvent.tab()
      await whenStable()

      expect(await screen.findByText(/Another program removed Jira/)).toBeTruthy()
      addConnection.mockResolvedValue({
        created: { source: 'jira', name: 'jira' },
        snapshot: { ...snapshot(), revision: 'r3' },
        providersResponse: { currentOs: 'linux', providers: providerInfo },
        installed: { epoch: 'mock-host', revision: 2 },
      })
      await userEvent.click(screen.getByRole('button', { name: 'Add it again' }))
      await whenStable()

      expect(addConnection).toHaveBeenCalledWith({ source: 'jira', label: 'Jira' })
      expect(opened).toHaveBeenCalledWith('jira/jira')
      expect(drafts.get('jira/jira')).toMatchObject({
        basedOn: 'r3',
        values: { baseUrl: { kind: 'replace', value: 'https://mine.example.org' } },
      })
      expect(drafts.get('jira/default')).toBeUndefined()
      expect(removed).not.toHaveBeenCalled()
    })

    it('keeps the draft where it was when the app on screen did not install the re-add', async () => {
      const gone: ProviderConfigSnapshotDto = { revision: 'r2', kinds: [], connections: [] }
      const drafts = new SourceDrafts()
      const { checkProviderConfig, addConnection, receiptStanding, opened, whenStable } =
        await setup({ load: loads(gone), drafts })
      checkProviderConfig.mockImplementation(request =>
        Promise.resolve(
          request.basedOn === 'r1'
            ? [
                {
                  field: { kind: 'connection' },
                  problem: 'removed',
                  message: 'Another program removed this source.',
                },
              ]
            : []
        )
      )
      const baseUrl = await screen.findByLabelText('Base URL')
      await userEvent.type(baseUrl, '/x')
      await userEvent.tab()
      await whenStable()
      addConnection.mockResolvedValue({
        created: { source: 'jira', name: 'jira' },
        snapshot: { ...snapshot(), revision: 'r3' },
        providersResponse: { currentOs: 'linux', providers: providerInfo },
        installed: null,
      })
      receiptStanding.mockReturnValue('notInstalled')

      await userEvent.click(await screen.findByRole('button', { name: 'Add it again' }))
      await whenStable()

      expect(opened).not.toHaveBeenCalled()
      expect(drafts.get('jira/default')).toBeDefined()
      expect(drafts.get('jira/jira')).toBeUndefined()
    })

    it('follows a newer snapshot when the source is shown again, so its changes can be kept', async () => {
      let reads = 0
      const load = vi.fn<() => Promise<ProviderConfigSnapshotDto>>(() => {
        reads++
        return Promise.resolve(reads === 1 ? snapshot() : { ...theirs(), revision: `r${reads}` })
      })
      const { saveProviderConfig, checkProviderConfig, switchProvider, whenStable } = await setup({
        load,
      })
      checkProviderConfig.mockImplementation(request =>
        Promise.resolve(request.basedOn === 'r1' ? changedHere() : [])
      )
      const baseUrl = await screen.findByLabelText('Base URL')
      await userEvent.clear(baseUrl)
      await userEvent.type(baseUrl, 'https://mine.example.org')
      await userEvent.tab()
      await whenStable()
      expect(await screen.findByText(/Now: https/)).toBeTruthy()

      // Away to another source and back: the pane reads a newer snapshot.
      await switchProvider(row(0))
      await whenStable()
      await switchProvider(row(1))
      await whenStable()
      await userEvent.click(await screen.findByRole('button', { name: 'Keep my changes' }))
      await whenStable()

      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty(
        'disabled',
        false
      )
      saveProviderConfig.mockResolvedValue(saveResponse(jiraFields))
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
      expect(saveProviderConfig).toHaveBeenLastCalledWith(
        expect.objectContaining({ basedOn: `r${reads}` })
      )
    })

    it('discards the draft of a removed source on request', async () => {
      const gone: ProviderConfigSnapshotDto = { revision: 'r2', kinds: [], connections: [] }
      const { checkProviderConfig, removed, whenStable } = await setup({ load: loads(gone) })
      checkProviderConfig.mockImplementation(request =>
        Promise.resolve(
          request.basedOn === 'r1'
            ? [
                {
                  field: { kind: 'connection' },
                  problem: 'removed',
                  message: 'Another program removed this source.',
                },
              ]
            : []
        )
      )
      const baseUrl = await screen.findByLabelText('Base URL')
      await userEvent.type(baseUrl, '/x')
      await userEvent.tab()
      await whenStable()

      await userEvent.click(await screen.findByRole('button', { name: 'Discard my changes' }))

      expect(removed).toHaveBeenCalled()
    })

    it('reads the settings again after a removal another program got in before', async () => {
      const { removeConnection, loadProviderConfig, whenStable } = await setup({
        load: loads({ ...snapshot(), revision: 'r2' }),
      })
      removeConnection.mockRejectedValueOnce(
        new BackendFailure('conflict', 'Jira was removed or replaced by another program.')
      )
      await screen.findByLabelText('Base URL')
      const remove = async (): Promise<void> => {
        await userEvent.click(screen.getByRole('button', { name: 'Remove source' }))
        await userEvent.click(
          within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Remove' })
        )
        await whenStable()
      }

      await remove()
      await vi.waitFor(() => expect(loadProviderConfig).toHaveBeenCalledTimes(2))
      await whenStable()
      removeConnection.mockResolvedValue(saveResponse(jiraFields))
      await remove()

      await vi.waitFor(() => expect(removeConnection).toHaveBeenCalledTimes(2))
      expect(removeConnection).toHaveBeenLastCalledWith({
        source: 'jira',
        name: 'default',
        basedOn: 'r2',
      })
    })

    it('keeps the draft, shows what changed, and saves it over the new settings on request', async () => {
      const load = vi
        .fn<() => Promise<ProviderConfigSnapshotDto>>()
        .mockResolvedValueOnce(snapshot())
        .mockResolvedValue(theirs())
      const { saveProviderConfig, checkProviderConfig, whenStable } = await setup({ load })
      // Another program saved after the draft's settings were read: the check knows it once the
      // save was refused, as the backend would.
      let refused = false
      saveProviderConfig.mockImplementationOnce(() => {
        refused = true
        return Promise.reject(
          new BackendFailure('conflict', 'Jira was changed by another program. Nothing was saved.')
        )
      })
      checkProviderConfig.mockImplementation(request =>
        Promise.resolve(refused && request.basedOn === 'r1' ? changedHere() : [])
      )
      const baseUrl = await screen.findByLabelText('Base URL')
      await userEvent.clear(baseUrl)
      await userEvent.type(baseUrl, 'https://mine.example.org')

      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
      await whenStable()

      expect(saveProviderConfig).toHaveBeenLastCalledWith(
        expect.objectContaining({ basedOn: 'r1' })
      )
      expect(await screen.findByText(/Now: https:\/\/theirs\.example\.org/)).toBeTruthy()
      expect(baseUrl).toHaveProperty('value', 'https://mine.example.org')
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)

      await userEvent.click(screen.getByRole('button', { name: 'Keep my changes' }))
      await whenStable()
      expect(screen.queryByText(/Now: https/)).toBeNull()
      saveProviderConfig.mockResolvedValue(saveResponse(jiraFields))
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenLastCalledWith({
        connection: { source: 'jira', name: 'default' },
        values: [{ key: 'baseUrl', value: 'https://mine.example.org' }],
        basedOn: 'r2',
      })
    })

    it('offers no saving over a source another program removed', async () => {
      const { saveProviderConfig, checkProviderConfig, whenStable } = await setup()
      let refused = false
      saveProviderConfig.mockImplementationOnce(() => {
        refused = true
        return Promise.reject(
          new BackendFailure('conflict', 'Jira was removed or replaced by another program.')
        )
      })
      checkProviderConfig.mockImplementation(() =>
        Promise.resolve(
          refused
            ? [
                {
                  field: { kind: 'connection' },
                  problem: 'removed',
                  message: 'Another program removed this source.',
                },
              ]
            : []
        )
      )
      const baseUrl = await screen.findByLabelText('Base URL')
      await userEvent.clear(baseUrl)
      await userEvent.type(baseUrl, 'https://mine.example.org')

      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
      await whenStable()

      expect(await screen.findByText('Another program removed this source.')).toBeTruthy()
      expect(screen.queryByRole('button', { name: 'Keep my changes' })).toBeNull()
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
      expect(baseUrl).toHaveProperty('value', 'https://mine.example.org')
    })
  })

  it('enables Save and Discard only while there are changes, and Discard drops them', async () => {
    await setup()
    const maxResults = await screen.findByLabelText('Max results')
    expect(screen.getByRole('button', { name: 'Discard' })).toHaveProperty('disabled', true)
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)

    await userEvent.clear(maxResults)
    await userEvent.type(maxResults, '25')
    expect(screen.getByRole('button', { name: 'Discard' })).toHaveProperty('disabled', false)

    await userEvent.click(screen.getByRole('button', { name: 'Discard' }))
    expect(screen.getByLabelText('Max results')).toHaveProperty('value', '50')
    expect(screen.getByRole('button', { name: 'Discard' })).toHaveProperty('disabled', true)
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
      connection: { source: 'jira', name: 'default' },
      values: [{ key: 'maxResults', value: '25' }],
      basedOn: 'r1',
    })
    expect(await screen.findByText('Jira saved')).toBeTruthy()
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
      connection: { source: 'jira', name: 'default' },
      values: [{ key: 'apiToken', value: 's3cret' }],
      basedOn: 'r1',
    })
    await screen.findByText('Jira saved')

    expect(token).toHaveProperty('placeholder', 'Saved; type to replace')
    await userEvent.click(screen.getByRole('button', { name: 'Clear saved API token' }))
    expect(screen.getByText('Not set')).toBeTruthy()
    expect(token).toHaveProperty('placeholder', '')
    expect(screen.queryByRole('button', { name: 'Clear saved API token' })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
    expect(saveProviderConfig).toHaveBeenLastCalledWith({
      connection: { source: 'jira', name: 'default' },
      values: [{ key: 'apiToken', value: null }],
      basedOn: 'r1',
    })
  })

  it('edits booleans through the select', async () => {
    const { saveProviderConfig } = await setup()
    saveProviderConfig.mockResolvedValue(saveResponse(jiraFields))

    await userEvent.click(await screen.findByRole('combobox'))
    await userEvent.click(await screen.findByRole('option', { name: 'False' }))
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

    expect(saveProviderConfig).toHaveBeenCalledWith({
      connection: { source: 'jira', name: 'default' },
      values: [{ key: 'verifyTls', value: 'false' }],
      basedOn: 'r1',
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
    expect(screen.queryByText('Jira saved')).toBeNull()
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
    expect(
      screen.queryByRole('button', { name: 'Save settings' })?.hasAttribute('disabled') ?? true
    ).toBe(true)
  })

  it('offers Clear and the saved placeholder only for secrets, not for saved plain values', async () => {
    await setup()
    const baseUrl = await screen.findByLabelText('Base URL')

    expect(baseUrl).toHaveProperty('placeholder', '')
    expect(screen.queryByRole('button', { name: 'Clear saved Base URL' })).toBeNull()
    expect(screen.getByRole('button', { name: 'Clear saved API token' })).toBeTruthy()
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

    await userEvent.click(screen.getByRole('button', { name: 'Clear saved Webhook' }))
    await userEvent.type(webhook, 'abc')
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

    expect(saveProviderConfig).toHaveBeenCalledWith({
      connection: { source: 'jira', name: 'default' },
      values: [{ key: 'webhook', value: 'abc' }],
      basedOn: 'r1',
    })
  })

  it('starts every source from its own values, and keeps unsaved edits for when it is back', async () => {
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
        revision: 'r1',
        kinds: [],
        connections: [state('git', gitFields), state('jira', jiraFields)],
      })
    const { saveProviderConfig, switchProvider } = await setup({ load: both })
    saveProviderConfig.mockRejectedValue(new BackendFailure('internal', 'write refused'))

    const baseUrl = await screen.findByLabelText('Base URL')
    await userEvent.clear(baseUrl)
    await userEvent.type(baseUrl, 'https://jira.internal')
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
    expect((await screen.findByRole('alert')).textContent).toContain('write refused')

    await switchProvider(row(0))

    expect(await screen.findByLabelText('Git root')).toHaveProperty('value', '/code')
    expect(screen.queryByRole('alert')).toBeNull()
    expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
    expect(screen.queryByLabelText('Base URL')).toBeNull()

    await switchProvider(row(1))

    // The edit the save failed for is still there, still unsaved, and so is why it failed.
    expect(await screen.findByLabelText('Base URL')).toHaveProperty(
      'value',
      'https://jira.internal'
    )
    expect(screen.getByRole('button', { name: 'Discard' })).toHaveProperty('disabled', false)
    expect(screen.getByRole('alert').textContent).toContain('write refused')

    await userEvent.click(screen.getByRole('button', { name: 'Discard' }))
    await switchProvider(row(0))
    await switchProvider(row(1))
    expect(await screen.findByLabelText('Base URL')).toHaveProperty(
      'value',
      'https://jira.example.com'
    )
    expect(screen.getByRole('button', { name: 'Discard' })).toHaveProperty('disabled', true)
  })

  describe('a save or removal that ends after switching to another source', () => {
    const gitRoot: ProviderConfigFieldStateDto = {
      key: 'gitRoot',
      label: 'Git root',
      required: false,
      shape: 'single',
      valueType: 'string',
      secret: false,
      isSet: true,
      value: '/code',
    }
    const both = (): ProviderConfigSnapshotDto => ({
      revision: 'r1',
      kinds: [],
      connections: [state('git', [gitRoot]), state('jira', jiraFields)],
    })

    /** Leaves an unsaved Git root on Git, then opens Jira. */
    async function editGitThenOpenJira(): Promise<Setup & { drafts: SourceDrafts }> {
      const drafts = new SourceDrafts()
      const parts = await setup({ load: () => Promise.resolve(both()), provider: row(0), drafts })
      await parts.whenStable()
      const root = screen.getByLabelText('Git root')
      await userEvent.clear(root)
      await userEvent.type(root, '/work')
      await parts.switchProvider(row(1))
      await parts.whenStable()
      expect(screen.getByLabelText('Base URL')).toBeTruthy()
      return { ...parts, drafts }
    }

    it('keeps the other source’s edits and names the saved one', async () => {
      const { saveProviderConfig, switchProvider, whenStable, drafts } = await editGitThenOpenJira()
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      saveProviderConfig.mockReturnValue(new Promise(resolve => (answer = resolve)))
      const baseUrl = screen.getByLabelText('Base URL')
      await userEvent.clear(baseUrl)
      await userEvent.type(baseUrl, 'https://jira.internal')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      await switchProvider(row(0))
      await whenStable()
      expect(screen.getByLabelText('Git root')).toHaveProperty('value', '/work')
      const toasted = nextToast()
      answer({
        snapshot: both(),
        providersResponse: { currentOs: 'linux', providers: providerInfo },
        installed: { epoch: 'mock-host', revision: 1 },
      })
      expect(await toasted).toBe('Jira saved')
      await whenStable()
      expect(screen.getByLabelText('Git root')).toHaveProperty('value', '/work')
      expect(screen.getByRole('button', { name: 'Discard' })).toHaveProperty('disabled', false)
      expect(drafts.get('git/default')?.values).toEqual({
        gitRoot: { kind: 'replace', value: '/work' },
      })
      expect(drafts.get('jira/default')).toBeUndefined()
    })

    it('shows a failed save on its own source only', async () => {
      const { saveProviderConfig, switchProvider, whenStable } = await editGitThenOpenJira()
      let refuse = (_: Error): void => undefined
      saveProviderConfig.mockReturnValue(new Promise((_, reject) => (refuse = reject)))
      const baseUrl = screen.getByLabelText('Base URL')
      await userEvent.clear(baseUrl)
      await userEvent.type(baseUrl, 'https://jira.internal')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      await switchProvider(row(0))
      refuse(new BackendFailure('internal', 'write refused'))
      await whenStable()
      expect(screen.queryByRole('alert')).toBeNull()
      expect(screen.getByLabelText('Git root')).toHaveProperty('value', '/work')
    })

    it('removes the source it asked about, even when another is shown by the answer', async () => {
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      let asked = (_: ConnectionId): void => undefined
      const sent = new Promise<ConnectionId>(resolve => (asked = resolve))
      const removeConnection = vi.fn((connection: ConnectionId) => {
        asked(connection)
        return new Promise<ProviderConfigSaveResponseDto>(resolve => (answer = resolve))
      })
      const removed = vi.fn()
      const drafts = new SourceDrafts()
      const { rerender, fixture } = await render(SettingsSourcePaneComponent, {
        inputs: { provider: row(0) },
        on: { removed },
        providers: [
          {
            provide: ConnectionSettings,
            useValue: {
              loadProviderConfig: (): Promise<ProviderConfigSnapshotDto> => Promise.resolve(both()),
              removeConnection,
            },
          },
          {
            provide: ActivityDataService,
            useValue: {
              receiptStanding: (): ReceiptStanding => 'current',
              hostEpoch: signal('mock-host'),
            },
          },
          { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => true } },
          { provide: SourceDrafts, useValue: drafts },
        ],
      })
      await fixture.whenStable()
      await userEvent.clear(screen.getByLabelText('Git root'))
      await userEvent.type(screen.getByLabelText('Git root'), '/work')
      await rerender({ partialUpdate: true, inputs: { provider: row(1) } })
      await fixture.whenStable()
      await userEvent.click(screen.getByRole('button', { name: 'Remove source' }))
      await fixture.whenStable()
      // Git is shown while the question about Jira is open.
      await rerender({ partialUpdate: true, inputs: { provider: row(0) } })
      await fixture.whenStable()
      await userEvent.click(
        within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Remove' })
      )
      expect(await sent).toEqual({ source: 'jira', name: 'default', basedOn: 'r1' })
      const toasted = nextToast()
      answer({
        snapshot: { revision: 'r2', kinds: [], connections: [state('git', [gitRoot])] },
        providersResponse: { currentOs: 'linux', providers: providerInfo },
        installed: { epoch: 'mock-host', revision: 1 },
      })

      expect(await toasted).toBe('Jira removed')
      await fixture.whenStable()
      expect(removed).not.toHaveBeenCalled()
      expect(drafts.get('git/default')?.values).toEqual({
        gitRoot: { kind: 'replace', value: '/work' },
      })
    })

    it('keeps an edit made after a save was sent, in a pane opened again', async () => {
      const drafts = new SourceDrafts()
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      let saving = (): void => undefined
      const sent = new Promise<void>(resolve => (saving = resolve))
      const saveProviderConfig = vi.fn(() => {
        saving()
        return new Promise<ProviderConfigSaveResponseDto>(resolve => (answer = resolve))
      })
      const providers = [
        {
          provide: ConnectionSettings,
          useValue: {
            loadProviderConfig: (): Promise<ProviderConfigSnapshotDto> => Promise.resolve(both()),
            saveProviderConfig,
          },
        },
        {
          provide: ActivityDataService,
          useValue: {
            receiptStanding: (): ReceiptStanding => 'current',
            hostEpoch: signal('mock-host'),
          },
        },
        { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => true } },
        { provide: SourceDrafts, useValue: drafts },
      ]
      const first = await render(SettingsSourcePaneComponent, {
        inputs: { provider: row(1) },
        providers,
      })
      await first.fixture.whenStable()
      await userEvent.clear(screen.getByLabelText('Base URL'))
      await userEvent.type(screen.getByLabelText('Base URL'), 'https://submitted.example.org')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
      await sent
      first.fixture.destroy()

      const second = TestBed.createComponent(SettingsSourcePaneComponent)
      second.componentRef.setInput('provider', row(1))
      await second.whenStable()
      const field = second.nativeElement.querySelector('input[id$="-baseUrl"]')
      if (!(field instanceof HTMLInputElement)) throw new Error('the pane shows the Base URL')
      await userEvent.clear(field)
      await userEvent.type(field, 'https://later.example.org')
      await second.whenStable()
      const toasted = nextToast()
      answer({
        snapshot: both(),
        providersResponse: { currentOs: 'linux', providers: providerInfo },
        installed: { epoch: 'mock-host', revision: 1 },
      })
      await toasted

      expect(drafts.get('jira/default')?.values['baseUrl']).toEqual({
        kind: 'replace',
        value: 'https://later.example.org',
      })
    })

    it('clears a saved draft that a pane opened again only showed', async () => {
      const drafts = new SourceDrafts()
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      let saving = (): void => undefined
      const sent = new Promise<void>(resolve => (saving = resolve))
      const saveProviderConfig = vi.fn(() => {
        saving()
        return new Promise<ProviderConfigSaveResponseDto>(resolve => (answer = resolve))
      })
      const providers = [
        {
          provide: ConnectionSettings,
          useValue: {
            loadProviderConfig: (): Promise<ProviderConfigSnapshotDto> => Promise.resolve(both()),
            saveProviderConfig,
          },
        },
        {
          provide: ActivityDataService,
          useValue: {
            receiptStanding: (): ReceiptStanding => 'current',
            hostEpoch: signal('mock-host'),
          },
        },
        { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => true } },
        { provide: SourceDrafts, useValue: drafts },
      ]
      const first = await render(SettingsSourcePaneComponent, {
        inputs: { provider: row(1) },
        providers,
      })
      await first.fixture.whenStable()
      await userEvent.clear(screen.getByLabelText('Base URL'))
      await userEvent.type(screen.getByLabelText('Base URL'), 'https://submitted.example.org')
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
      await sent
      first.fixture.destroy()

      // Opened again while the save is out: the draft shows, and nothing is edited.
      const second = TestBed.createComponent(SettingsSourcePaneComponent)
      second.componentRef.setInput('provider', row(1))
      await second.whenStable()
      const toasted = nextToast()
      answer({
        snapshot: both(),
        providersResponse: { currentOs: 'linux', providers: providerInfo },
        installed: { epoch: 'mock-host', revision: 1 },
      })
      await toasted

      expect(drafts.get('jira/default')).toBeUndefined()
    })

    it('keeps the other source’s edits after a removal and stays on it', async () => {
      let answer = (_: ProviderConfigSaveResponseDto): void => undefined
      let asked = (_: ConnectionId): void => undefined
      const sent = new Promise<ConnectionId>(resolve => (asked = resolve))
      const removeConnection = vi.fn((connection: ConnectionId) => {
        asked(connection)
        return new Promise<ProviderConfigSaveResponseDto>(resolve => (answer = resolve))
      })
      const removed = vi.fn()
      const drafts = new SourceDrafts()
      const { rerender, fixture } = await render(SettingsSourcePaneComponent, {
        inputs: { provider: row(0) },
        on: { removed },
        providers: [
          {
            provide: ConnectionSettings,
            useValue: {
              loadProviderConfig: (): Promise<ProviderConfigSnapshotDto> => Promise.resolve(both()),
              removeConnection,
            },
          },
          {
            provide: ActivityDataService,
            useValue: {
              receiptStanding: (): ReceiptStanding => 'current',
              hostEpoch: signal('mock-host'),
            },
          },
          { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => true } },
          { provide: SourceDrafts, useValue: drafts },
        ],
      })
      await fixture.whenStable()
      const root = screen.getByLabelText('Git root')
      await userEvent.clear(root)
      await userEvent.type(root, '/work')
      await rerender({ partialUpdate: true, inputs: { provider: row(1) } })
      await fixture.whenStable()
      await userEvent.click(screen.getByRole('button', { name: 'Remove source' }))
      await fixture.whenStable()
      await userEvent.click(
        within(screen.getByRole('alertdialog')).getByRole('button', { name: 'Remove' })
      )
      expect(await sent).toEqual({ source: 'jira', name: 'default', basedOn: 'r1' })

      await rerender({ partialUpdate: true, inputs: { provider: row(0) } })
      await fixture.whenStable()
      expect(screen.getByLabelText('Git root')).toHaveProperty('value', '/work')
      const toasted = nextToast()
      answer({
        snapshot: { revision: 'r2', kinds: [], connections: [state('git', [gitRoot])] },
        providersResponse: { currentOs: 'linux', providers: providerInfo },
        installed: { epoch: 'mock-host', revision: 1 },
      })
      expect(await toasted).toBe('Jira removed')
      await fixture.whenStable()
      expect(removed).not.toHaveBeenCalled()
      expect(screen.getByLabelText('Git root')).toHaveProperty('value', '/work')
      expect(drafts.get('git/default')?.values).toEqual({
        gitRoot: { kind: 'replace', value: '/work' },
      })
    })
  })

  it('keeps unsaved edits once the pane is gone, until they are saved', async () => {
    const { saveProviderConfig } = await setup()
    const drafts = TestBed.inject(SourceDrafts)
    const maxResults = await screen.findByLabelText('Max results')
    await userEvent.clear(maxResults)
    await userEvent.type(maxResults, '25')
    expect(drafts.get('jira/default')?.values).toEqual({
      maxResults: { kind: 'replace', value: '25' },
    })

    saveProviderConfig.mockResolvedValue(
      saveResponse(
        jiraFields.map(field => (field.key === 'maxResults' ? { ...field, value: '25' } : field))
      )
    )
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))
    expect(await screen.findByText('Jira saved')).toBeTruthy()
    expect(drafts.get('jira/default')).toBeUndefined()
  })

  it('shows the edits left in a draft when the pane opens', async () => {
    const drafts = new SourceDrafts()
    drafts.set('jira/default', {
      basedOn: 'r1',
      label: null,
      values: { baseUrl: { kind: 'replace', value: 'https://jira.internal' } },
      lists: {},
    })
    await setup({ drafts })

    expect(await screen.findByLabelText('Base URL')).toHaveProperty(
      'value',
      'https://jira.internal'
    )
    expect(screen.getByLabelText('Max results')).toHaveProperty('value', '50')
    expect(screen.getByRole('button', { name: 'Discard' })).toHaveProperty('disabled', false)
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
        connection: { source: 'jira', name: 'default' },
        values: [{ key: 'GIT_DIRS', value: ['/home/me', '/srv/repos'] }],
        basedOn: 'r1',
      })
    })

    it('saves an emptied list as none', async () => {
      const { saveProviderConfig } = await setupList(gitDirs())

      await userEvent.click(screen.getByRole('button', { name: 'Remove /home/me' }))
      expect(screen.getByText('None')).toBeTruthy()
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenCalledWith({
        connection: { source: 'jira', name: 'default' },
        values: [{ key: 'GIT_DIRS', value: [] }],
        basedOn: 'r1',
      })
    })

    it('shows an item’s issue on its row, blank rows not counted', async () => {
      const { checkProviderConfig } = await setupList(
        gitDirs({ itemType: 'url', items: ['https://a.example'] })
      )
      checkProviderConfig.mockResolvedValue([
        {
          field: { kind: 'item', key: 'GIT_DIRS', index: 1 },
          problem: 'invalid',
          message: 'Enter an http or https address.',
        },
      ])

      await userEvent.click(screen.getByRole('button', { name: 'Add' }))
      await userEvent.click(screen.getByRole('button', { name: 'Add' }))
      await userEvent.type(screen.getByLabelText('Git Dirs 3'), 'not a url')
      await userEvent.tab()

      const row = await screen.findByLabelText('Git Dirs 3')
      expect(row.getAttribute('aria-invalid')).toBe('true')
      expect(screen.getByLabelText('Git Dirs 2').getAttribute('aria-invalid')).not.toBe('true')
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
    })

    it('shows a list another program changed as a whole, and holds Save until it is resolved', async () => {
      const { checkProviderConfig, whenStable } = await setupList(
        gitDirs({ isSet: true, items: ['/work'] })
      )
      checkProviderConfig.mockImplementation(request =>
        Promise.resolve(
          request.basedOn === 'r1'
            ? [
                {
                  field: { kind: 'value', key: 'GIT_DIRS' },
                  problem: 'changed',
                  message: 'Another program changed this since you started editing.',
                },
              ]
            : []
        )
      )

      await userEvent.type(screen.getByLabelText('Git Dirs 1'), '/more')
      await userEvent.tab()
      await whenStable()

      expect(await screen.findByText(/Another program changed this.*Now: \/work\./)).toBeTruthy()
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty('disabled', true)
      await userEvent.click(screen.getByRole('button', { name: 'Keep my changes' }))
      await whenStable()
      expect(screen.getByRole('button', { name: 'Save settings' })).toHaveProperty(
        'disabled',
        false
      )
    })

    it('resets a configured list to its default', async () => {
      const { saveProviderConfig } = await setupList(gitDirs({ isSet: true, items: ['/work'] }))

      await userEvent.click(screen.getByRole('button', { name: 'Use default' }))
      expect(screen.getByText('The default applies after saving.')).toBeTruthy()
      await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

      expect(saveProviderConfig).toHaveBeenCalledWith({
        connection: { source: 'jira', name: 'default' },
        values: [{ key: 'GIT_DIRS', value: null }],
        basedOn: 'r1',
      })
    })
  })
})

describe('SettingsSourcePaneComponent connections', () => {
  const connectionsSnapshot = (): ProviderConfigSnapshotDto => ({
    revision: 'r1',
    kinds: [],
    connections: [state('git', []), state('jira', jiraFields)],
  })

  async function setupConnections(): Promise<{
    saveProviderConfig: ReturnType<typeof vi.fn>
    removeConnection: ReturnType<typeof vi.fn>
    removed: ReturnType<typeof vi.fn>
    receiptStanding: ReturnType<typeof vi.fn<(receipt: InstallReceipt | null) => ReceiptStanding>>
  }> {
    const receiptStanding = vi.fn<(receipt: InstallReceipt | null) => ReceiptStanding>(
      () => 'current'
    )
    const saveProviderConfig = vi.fn().mockResolvedValue({
      snapshot: connectionsSnapshot(),
      providersResponse: { currentOs: 'linux', providers: providerInfo },
      installed: { epoch: 'mock-host', revision: 1 },
    })
    const removeConnection = vi.fn().mockResolvedValue({
      snapshot: connectionsSnapshot(),
      providersResponse: { currentOs: 'linux', providers: providerInfo },
      installed: { epoch: 'mock-host', revision: 1 },
    })
    const removed = vi.fn()
    await render(SettingsSourcePaneComponent, {
      inputs: { provider: row(1) },
      on: { removed },
      providers: [
        {
          provide: ConnectionSettings,
          useValue: {
            loadProviderConfig: (): Promise<ProviderConfigSnapshotDto> =>
              Promise.resolve(connectionsSnapshot()),
            saveProviderConfig,
            removeConnection,
          },
        },
        {
          provide: ActivityDataService,
          useValue: {
            receiptStanding,
            hostEpoch: signal('mock-host'),
          },
        },
        { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => true } },
      ],
    })
    await screen.findByLabelText('Label')
    return { saveProviderConfig, removeConnection, removed, receiptStanding }
  }

  it('leaves the draft and the section of a host that replaced the one a removal reached', async () => {
    const { removeConnection, removed, receiptStanding } = await setupConnections()
    const drafts = TestBed.inject(SourceDrafts)
    let answer = (_: ProviderConfigSaveResponseDto): void => undefined
    removeConnection.mockImplementationOnce(() => new Promise(resolve => (answer = resolve)))
    await userEvent.click(screen.getByRole('button', { name: 'Remove source' }))
    const question = await screen.findByRole('alertdialog')
    await userEvent.click(within(question).getByRole('button', { name: 'Remove' }))
    await vi.waitFor(() => expect(removeConnection).toHaveBeenCalled())
    // Host b replaces host a, and the user edits b's connection of the same name.
    receiptStanding.mockReturnValue('otherHost')
    drafts.set('jira/default', { basedOn: 'r1', label: 'Host B edit', values: {}, lists: {} })

    const toasted = nextToast()
    answer({
      snapshot: connectionsSnapshot(),
      providersResponse: { currentOs: 'linux', providers: providerInfo },
      installed: { epoch: 'a', revision: 1 },
    })
    await toasted

    expect(drafts.get('jira/default')?.label).toBe('Host B edit')
    expect(removed).not.toHaveBeenCalled()
  })

  it('leaves the draft and the section of a removal written but never installed (B70)', async () => {
    const { removeConnection, removed, receiptStanding } = await setupConnections()
    receiptStanding.mockImplementation(receipt => (receipt === null ? 'notInstalled' : 'current'))
    const drafts = TestBed.inject(SourceDrafts)
    drafts.set('jira/default', { basedOn: 'r1', label: 'Edited meanwhile', values: {}, lists: {} })
    // The file holds the removal, but the app quit before it installed it.
    removeConnection.mockResolvedValueOnce({
      snapshot: connectionsSnapshot(),
      providersResponse: { currentOs: 'linux', providers: providerInfo },
      installed: null,
    })

    await userEvent.click(screen.getByRole('button', { name: 'Remove source' }))
    const question = await screen.findByRole('alertdialog')
    const toasted = nextToast()
    await userEvent.click(within(question).getByRole('button', { name: 'Remove' }))
    await toasted

    expect(receiptStanding).toHaveBeenCalledWith(null)
    expect(drafts.get('jira/default')?.label).toBe('Edited meanwhile')
    expect(removed).not.toHaveBeenCalled()
  })

  it('keeps an edit made while the removal was out', async () => {
    const { removeConnection, removed } = await setupConnections()
    const drafts = TestBed.inject(SourceDrafts)
    let answer = (_: ProviderConfigSaveResponseDto): void => undefined
    removeConnection.mockImplementationOnce(() => new Promise(resolve => (answer = resolve)))
    await userEvent.click(screen.getByRole('button', { name: 'Remove source' }))
    const question = await screen.findByRole('alertdialog')
    await userEvent.click(within(question).getByRole('button', { name: 'Remove' }))
    await vi.waitFor(() => expect(removeConnection).toHaveBeenCalled())
    drafts.set('jira/default', { basedOn: 'r1', label: 'Added again', values: {}, lists: {} })

    const toasted = nextToast()
    answer({
      snapshot: connectionsSnapshot(),
      providersResponse: { currentOs: 'linux', providers: providerInfo },
      installed: { epoch: 'mock-host', revision: 2 },
    })
    await toasted

    expect(drafts.get('jira/default')?.label).toBe('Added again')
    expect(removed).toHaveBeenCalled()
  })

  it('renames a connection with its label', async () => {
    const { saveProviderConfig } = await setupConnections()

    const label = screen.getByLabelText('Label')
    await userEvent.clear(label)
    await userEvent.type(label, 'Company Jira')
    await userEvent.click(screen.getByRole('button', { name: 'Save settings' }))

    expect(saveProviderConfig).toHaveBeenCalledWith({
      connection: { source: 'jira', name: 'default' },
      label: 'Company Jira',
      values: [],
      basedOn: 'r1',
    })
  })

  it('removes a connection after confirming, with its unsaved edits', async () => {
    const { removeConnection, removed } = await setupConnections()
    await userEvent.type(screen.getByLabelText('Label'), ' 2')
    expect(TestBed.inject(SourceDrafts).get('jira/default')).toBeDefined()

    await userEvent.click(screen.getByRole('button', { name: 'Remove source' }))
    const dialog = await screen.findByRole('alertdialog')
    expect(dialog.textContent).toContain('Remove Jira?')
    expect(removeConnection).not.toHaveBeenCalled()
    await userEvent.click(within(dialog).getByRole('button', { name: 'Remove' }))

    await vi.waitFor(() =>
      expect(removeConnection).toHaveBeenCalledWith({
        source: 'jira',
        name: 'default',
        basedOn: 'r1',
      })
    )
    expect(removed).toHaveBeenCalled()
    expect(await screen.findByText('Jira removed')).toBeTruthy()
    expect(TestBed.inject(SourceDrafts).get('jira/default')).toBeUndefined()
  })

  it('keeps a connection when removing is cancelled', async () => {
    const { removeConnection } = await setupConnections()

    await userEvent.click(screen.getByRole('button', { name: 'Remove source' }))
    const dialog = await screen.findByRole('alertdialog')
    await userEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }))

    await vi.waitFor(() => expect(screen.queryByRole('alertdialog')).toBeNull())
    expect(removeConnection).not.toHaveBeenCalled()
  })
})
