import { TestBed } from '@angular/core/testing'
import { emit } from '@tauri-apps/api/event'
import { mockIPC } from '@tauri-apps/api/mocks'
import { clearTauriMocks } from '../../testing/tauri-mocks'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { collectingLog, exportRequest, MockBackendTransport } from '../../testing/activity-fixtures'
import type { RuntimeState } from '../../generated/activity'
import { BackendOperations } from './backend-contract'
import { BackendFailure } from './backend-failure'
import type { RuntimeStreamHandlers } from './runtime-stream'
import { TauriBackendTransport } from './tauri-backend-transport'

interface Invocation {
  readonly command: string
  readonly args: unknown
}

/** Answers every Tauri command with `reply` and records what was invoked. */
function mockInvoke(reply: (command: string) => unknown = () => undefined): Invocation[] {
  const invocations: Invocation[] = []
  mockIPC((command, args) => {
    invocations.push({ command, args })
    return reply(command)
  })
  return invocations
}

describe('TauriBackendTransport', () => {
  afterEach(() => {
    clearTauriMocks()
  })

  function setup(): TauriBackendTransport {
    return TestBed.inject(TauriBackendTransport)
  }

  it('invokes reads by command name without a payload', async () => {
    const invocations = mockInvoke(() => ({ entries: [] }))

    await expect(setup().call(BackendOperations.logs, undefined)).resolves.toEqual({ entries: [] })

    expect(invocations).toEqual([{ command: 'get_logs', args: {} }])
  })

  it('passes the request as { request }', async () => {
    const invocations = mockInvoke(() => collectingLog)
    const request = { start: '2026-05-17T00:00:00Z', end: '2026-05-19T00:00:00Z' }

    await setup().call(BackendOperations.collect, request)

    expect(invocations).toEqual([{ command: 'collect', args: { request } }])
  })

  it('invokes the desktop-only commands', async () => {
    const snapshot = { revision: 'r1', kinds: [], connections: [] }
    const invocations = mockInvoke(command => {
      if (command === 'get_provider_config') return snapshot
      if (command === 'save_provider_config') return { snapshot, installed: null }
      return undefined
    })
    const transport = setup()

    await transport.call(BackendOperations.providerConfig, undefined)
    await transport.call(BackendOperations.saveProviderConfig, {
      connection: { source: 'git', name: 'default' },
      values: [],
      basedOn: 'snapshot',
    })
    await transport.call(BackendOperations.clearLogs, undefined)

    expect(invocations).toEqual([
      { command: 'get_provider_config', args: {} },
      {
        command: 'save_provider_config',
        args: {
          request: {
            connection: { source: 'git', name: 'default' },
            values: [],
            basedOn: 'snapshot',
          },
        },
      },
      { command: 'clear_logs', args: {} },
    ])
  })

  const checkRequest = {
    connection: { source: 'jira', name: 'default' } as const,
    values: [],
    basedOn: 'r1',
  }
  const createToken = {
    kind: 'createToken',
    url: 'https://jira.example.org/secure/ViewProfile.jspa',
    instructions: 'Create a personal access token.',
  }

  it.each([undefined, createToken])('reads a settings action of %j', async action => {
    const field = { key: 'JIRA_TOKEN', ...(action && { fixAction: action }) }
    const snapshot = { revision: 'r1', kinds: [], connections: [{ fields: [field] }] }
    const check = { issues: [], ...(action && { fieldFix: { key: field.key, action } }) }
    mockInvoke(command => (command === 'check_provider_config' ? check : snapshot))

    await expect(setup().call(BackendOperations.providerConfig, undefined)).resolves.toEqual(
      snapshot
    )
    await expect(
      setup().call(BackendOperations.checkProviderConfig, checkRequest)
    ).resolves.toEqual(check)
  })

  it.each([
    { ...createToken, url: 'javascript:alert(1)' },
    { ...createToken, classicUrl: 'https://user@github.example.org/settings/tokens/new' },
    { kind: 'createToken', url: createToken.url },
    { kind: 'local', target: { kind: 'openApp', app: 'outlook' } },
  ])('rejects a settings action of %j', async action => {
    const field = { key: 'JIRA_TOKEN', fixAction: action }
    mockInvoke(command =>
      command === 'check_provider_config'
        ? { issues: [], fieldFix: { key: field.key, action } }
        : { revision: 'r1', kinds: [], connections: [{ fields: [field] }] }
    )

    await expect(setup().call(BackendOperations.providerConfig, undefined)).rejects.toMatchObject({
      kind: 'internal',
    })
    await expect(
      setup().call(BackendOperations.checkProviderConfig, checkRequest)
    ).rejects.toMatchObject({
      kind: 'internal',
    })
  })

  it('saves exports through save_export, and says whether the user saved', async () => {
    const invocations = mockInvoke(() => 'cancelled')

    await expect(setup().saveExport(exportRequest)).resolves.toBe('cancelled')

    expect(invocations).toEqual([{ command: 'save_export', args: { request: exportRequest } }])
  })

  it('has no command for a browser-only operation', async () => {
    const invocations = mockInvoke()

    await expect(setup().call(BackendOperations.renderExport, exportRequest)).rejects.toMatchObject(
      { kind: 'unsupported' }
    )
    expect(invocations).toEqual([])
  })

  it('hands over the launch link once it listens, then each link that arrives', async () => {
    const commands: string[] = []
    mockIPC(
      command => {
        commands.push(command)
        return command === 'take_launch_deep_link' ? 'activity-log://timeline' : undefined
      },
      { shouldMockEvents: true }
    )
    const opened: string[] = []

    await setup().listenForDeepLinks(link => opened.push(link))
    await emit('deep-link', 'activity-log://day/2026-11-17?q=standup')

    expect(commands.filter(command => command === 'take_launch_deep_link')).toHaveLength(1)
    expect(opened).toEqual(['activity-log://timeline', 'activity-log://day/2026-11-17?q=standup'])
  })

  it('opens the retained link before a live link received while the answer is in flight', async () => {
    const launch = 'activity-log://timeline'
    const live = 'activity-log://day/2026-11-17'
    mockIPC(
      async command => {
        if (command !== 'take_launch_deep_link') return null
        await emit('deep-link', live)
        return launch
      },
      { shouldMockEvents: true }
    )
    const opened: string[] = []

    await setup().listenForDeepLinks(link => opened.push(link))

    expect(opened).toEqual([launch, live])
  })

  it('still opens the links it held when the launch link cannot be read', async () => {
    const live = 'activity-log://day/2026-11-17'
    mockIPC(
      async command => {
        if (command !== 'take_launch_deep_link') return null
        await emit('deep-link', live)
        throw { kind: 'internal', error: 'no inbox' }
      },
      { shouldMockEvents: true }
    )
    const opened: string[] = []

    await expect(setup().listenForDeepLinks(link => opened.push(link))).rejects.toMatchObject({
      kind: 'internal',
    })

    expect(opened).toEqual([live])
  })

  it('opens nothing at launch without a launch link', async () => {
    mockIPC(() => null, { shouldMockEvents: true })
    const opened: string[] = []

    await setup().listenForDeepLinks(link => opened.push(link))

    expect(opened).toEqual([])
  })

  it('opens links through open_url', async () => {
    const invocations = mockInvoke()

    await setup().openUrl('https://example.org/')

    expect(invocations).toEqual([
      { command: 'open_url', args: { request: 'https://example.org/' } },
    ])
  })

  it('sends only the connection and the typed native recovery target', async () => {
    const invocations = mockInvoke()
    const request = {
      connection: { source: 'outlook', name: 'work' },
      target: { kind: 'openApp', app: 'outlook' },
    } as const

    await setup().call(BackendOperations.openLocalFix, request)

    expect(invocations).toEqual([{ command: 'open_local_fix', args: { request } }])
  })

  it('rejects with the kind a command refused with', async () => {
    mockInvoke(() => {
      throw { kind: 'busy', error: 'The settings file is busy; try again.' }
    })

    const failure = setup().call(BackendOperations.clearLogs, undefined)

    await expect(failure).rejects.toBeInstanceOf(BackendFailure)
    await expect(failure).rejects.toMatchObject({
      kind: 'busy',
      message: 'The settings file is busy; try again.',
    })
  })

  it('reads Tauri’s own string refusal as internal, with its text', async () => {
    mockInvoke(() => {
      throw 'invalid args `request` for command `collect`'
    })

    await expect(setup().call(BackendOperations.runtimeState, undefined)).rejects.toMatchObject({
      kind: 'internal',
      message: 'invalid args `request` for command `collect`',
    })
  })

  it('never prints an object outside the contract as [object Object]', async () => {
    mockInvoke(() => {
      throw { code: 7 }
    })

    const failure: unknown = await setup()
      .call(BackendOperations.runtimeState, undefined)
      .catch((error: unknown) => error)

    expect(failure).toMatchObject({ kind: 'internal' })
    expect(String(failure)).not.toContain('[object Object]')
  })

  it('loads reads as resources and surfaces failures as BackendFailures', async () => {
    mockInvoke(() => {
      throw { kind: 'internal', error: 'boom' }
    })

    const logs = TestBed.runInInjectionContext(() => setup().resource(BackendOperations.logs))
    TestBed.tick()

    await vi.waitFor(() => expect(logs.error()).toBeInstanceOf(BackendFailure))
    expect(logs.error()).toMatchObject({ kind: 'internal', message: 'boom' })
  })

  it('rejects with a BackendFailure when the shell refuses the deep-link listener', async () => {
    mockInvoke(command => {
      if (command === 'plugin:event|listen') throw 'event.listen not allowed'
      return null
    })

    const refused = setup().listenForDeepLinks(() => undefined)

    await expect(refused).rejects.toBeInstanceOf(BackendFailure)
    await expect(refused).rejects.toMatchObject({
      kind: 'internal',
      cause: 'event.listen not allowed',
    })
  })

  it('invokes nothing for a call aborted before it is sent, and rejects with the reason', async () => {
    const invocations = mockInvoke()
    const controller = new AbortController()
    const reason = new Error('stopped')
    controller.abort(reason)

    const call = setup().call(BackendOperations.clearLogs, undefined, controller.signal)

    expect(call).toBeInstanceOf(Promise)
    await expect(call).rejects.toBe(reason)
    expect(invocations).toEqual([])
  })

  describe('the state stream', () => {
    /** A pair of host `epoch` at `revision`. */
    function pairOf(epoch: string, revision: number): RuntimeState {
      const host = new MockBackendTransport()
      host.epoch = epoch
      const { activity, providers } = host.runtimeState()
      return { activity: { ...activity, revision }, providers: { ...providers, revision } }
    }

    function recording(): { handlers: RuntimeStreamHandlers; seen: unknown[] } {
      const seen: unknown[] = []
      return {
        seen,
        handlers: {
          state: pair => seen.push(['state', pair.activity.epoch, pair.activity.revision]),
          activity: update => seen.push(['activity', update.epoch, update.revision]),
          providers: update => seen.push(['providers', update.epoch, update.revision]),
          lost: failure => seen.push(['lost', failure]),
          progress: () => undefined,
        },
      }
    }

    it('reads the pair once it listens, then hands over what came meanwhile and later', async () => {
      const pair = pairOf('a', 3)
      mockIPC(
        async command => {
          if (command !== 'get_runtime_state') return null
          // Sent while the pair is on its way: one of this host, one of a host before it.
          await emit('activity', { ...pair.activity, revision: 4 })
          await emit('providers', { ...pairOf('old', 9).providers })
          return pair
        },
        { shouldMockEvents: true }
      )
      const { handlers, seen } = recording()

      setup().openRuntimeStream(handlers)
      await vi.waitFor(() => expect(seen).toHaveLength(2))
      await emit('providers', { ...pair.providers, revision: 5 })

      expect(seen).toEqual([
        ['state', 'a', 3],
        ['activity', 'a', 4],
        ['providers', 'a', 5],
      ])
    })

    it('ends with the read’s failure and delivers nothing after it', async () => {
      mockIPC(
        command => {
          if (command === 'get_runtime_state') throw { kind: 'internal', error: 'boom' }
          return null
        },
        { shouldMockEvents: true }
      )
      const { handlers, seen } = recording()

      setup().openRuntimeStream(handlers)
      await vi.waitFor(() => expect(seen).toHaveLength(1))
      await emit('activity', pairOf('a', 4).activity)

      expect(seen).toEqual([
        ['lost', expect.objectContaining({ kind: 'internal', message: 'boom' })],
      ])
      expect(seen[0]).toEqual(['lost', expect.any(BackendFailure)])
    })

    it('ends with a refused listener as a BackendFailure, its cause kept', async () => {
      mockInvoke(command => {
        if (command === 'plugin:event|listen') throw 'event.listen not allowed'
        return null
      })
      const { handlers, seen } = recording()

      setup().openRuntimeStream(handlers)

      await vi.waitFor(() => expect(seen).toHaveLength(1))
      expect(seen[0]).toEqual([
        'lost',
        expect.objectContaining({ kind: 'internal', cause: 'event.listen not allowed' }),
      ])
    })

    it('keeps only the latest message of each event until the pair is in', async () => {
      const pair = pairOf('a', 3)
      mockIPC(
        async command => {
          if (command !== 'get_runtime_state') return null
          await emit('activity', { ...pair.activity, revision: 4 })
          await emit('activity', { ...pair.activity, revision: 5 })
          await emit('state', pairOf('a', 6))
          await emit('state', pairOf('a', 7))
          return pair
        },
        { shouldMockEvents: true }
      )
      const { handlers, seen } = recording()

      setup().openRuntimeStream(handlers)

      await vi.waitFor(() => expect(seen).toHaveLength(3))
      expect(seen).toEqual([
        ['state', 'a', 3],
        ['state', 'a', 7],
        ['activity', 'a', 5],
      ])
    })

    it('ends at a payload it cannot read, before the pair or after', async () => {
      const pair = pairOf('a', 3)
      mockIPC(
        async command => {
          if (command !== 'get_runtime_state') return null
          await emit('activity', { ...pair.activity, state: {} })
          return pair
        },
        { shouldMockEvents: true }
      )
      const { handlers, seen } = recording()

      setup().openRuntimeStream(handlers)

      await vi.waitFor(() => expect(seen).toHaveLength(1))
      expect(seen).toEqual([['lost', expect.objectContaining({ kind: 'internal' })]])
    })

    it('ends at a handshake read it cannot read', async () => {
      mockIPC(command => (command === 'get_runtime_state' ? {} : null), {
        shouldMockEvents: true,
      })
      const { handlers, seen } = recording()

      setup().openRuntimeStream(handlers)

      await vi.waitFor(() => expect(seen).toHaveLength(1))
      expect(seen).toEqual([['lost', expect.objectContaining({ kind: 'internal' })]])
    })

    it('removes a listener registered after the stream closed, once', async () => {
      const unlistened: unknown[] = []
      const registrations: (() => void)[] = []
      let ids = 0
      mockIPC(command => {
        if (command === 'plugin:event|unlisten') {
          unlistened.push(command)
          return null
        }
        if (command !== 'plugin:event|listen') return null
        const id = ++ids
        // The third registration is answered only after the stream closed.
        if (id < 3) return id
        return new Promise<number>(resolve => registrations.push(() => resolve(id)))
      })
      const stream = setup().openRuntimeStream(recording().handlers)
      await vi.waitFor(() => expect(registrations).toHaveLength(1))

      stream.close()
      expect(unlistened).toHaveLength(0)
      await vi.waitFor(() => expect(unlistened).toHaveLength(2))
      registrations[0]?.()
      await vi.waitFor(() => expect(unlistened).toHaveLength(3))
      stream.close()
      await new Promise(resolve => setTimeout(resolve, 0))

      expect(unlistened).toHaveLength(3)
    })

    it('delivers nothing once closed, also a pair that comes after', async () => {
      const handshake: { answer?: (pair: RuntimeState) => void } = {}
      mockIPC(
        command =>
          command === 'get_runtime_state'
            ? new Promise<RuntimeState>(resolve => (handshake.answer = resolve))
            : null,
        { shouldMockEvents: true }
      )
      const { handlers, seen } = recording()
      const stream = setup().openRuntimeStream(handlers)
      // Listening, and the pair asked for.
      await vi.waitFor(() => expect(handshake.answer).toBeDefined())

      stream.close()
      handshake.answer?.(pairOf('a', 3))
      await emit('activity', pairOf('a', 4).activity)
      await new Promise(resolve => setTimeout(resolve, 0))

      expect(seen).toEqual([])
    })
  })
})
