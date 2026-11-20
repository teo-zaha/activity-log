import { TestBed } from '@angular/core/testing'
import { emit } from '@tauri-apps/api/event'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { collectingLog, exportRequest } from '../../testing/activity-fixtures'
import { BackendOperations } from './backend-contract'
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
    clearMocks()
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

  it('saves exports through save_export', async () => {
    const invocations = mockInvoke()
    await setup().saveExport(exportRequest)

    expect(invocations).toEqual([{ command: 'save_export', args: { request: exportRequest } }])
  })

  it('has no command for a browser-only operation', async () => {
    const invocations = mockInvoke()

    await expect(setup().call(BackendOperations.renderExport, exportRequest)).rejects.toThrow(
      'Export is only available in the browser.'
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

  it('turns the string rejection of a command into an Error', async () => {
    mockInvoke(() => {
      throw 'No data yet'
    })

    const failure = setup().call(BackendOperations.runtimeState, undefined)

    await expect(failure).rejects.toBeInstanceOf(Error)
    await expect(failure).rejects.toThrow('No data yet')
  })

  it('loads reads as resources and surfaces failures as Errors', async () => {
    mockInvoke(() => {
      throw 'boom'
    })

    const logs = TestBed.runInInjectionContext(() => setup().resource(BackendOperations.logs))
    TestBed.tick()

    await vi.waitFor(() => expect(logs.error()).toEqual(new Error('boom')))
  })
})
