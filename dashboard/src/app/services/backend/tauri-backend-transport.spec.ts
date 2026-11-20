import { TestBed } from '@angular/core/testing'
import { emit } from '@tauri-apps/api/event'
import { mockIPC } from '@tauri-apps/api/mocks'
import { clearTauriMocks } from '../../testing/tauri-mocks'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { collectingLog, exportRequest } from '../../testing/activity-fixtures'
import { BackendOperations } from './backend-contract'
import { BackendFailure } from './backend-failure'
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
    const invocations = mockInvoke()
    const transport = setup()

    await transport.call(BackendOperations.providerConfig, undefined)
    await transport.call(BackendOperations.saveProviderConfig, {
      connection: { source: 'git', name: 'default' },
      values: [],
    })
    await transport.call(BackendOperations.clearLogs, undefined)

    expect(invocations).toEqual([
      { command: 'get_provider_config', args: {} },
      {
        command: 'save_provider_config',
        args: { request: { connection: { source: 'git', name: 'default' }, values: [] } },
      },
      { command: 'clear_logs', args: {} },
    ])
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
})
