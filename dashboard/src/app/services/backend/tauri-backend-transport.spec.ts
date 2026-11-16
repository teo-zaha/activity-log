import { TestBed } from '@angular/core/testing'
import { clearMocks, mockIPC } from '@tauri-apps/api/mocks'
import { afterEach, describe, expect, it, vi } from 'vitest'
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
    const invocations = mockInvoke(() => ({ collecting: true }))
    const request = { start: '2026-05-17T00:00:00Z', end: '2026-05-19T00:00:00Z' }

    await setup().call(BackendOperations.collect, request)

    expect(invocations).toEqual([{ command: 'collect', args: { request } }])
  })

  it('invokes the desktop-only commands', async () => {
    const invocations = mockInvoke()
    const transport = setup()

    await transport.call(BackendOperations.providerConfig, undefined)
    await transport.call(BackendOperations.saveProviderConfig, { source: 'git', values: [] })
    await transport.call(BackendOperations.clearLogs, undefined)

    expect(invocations).toEqual([
      { command: 'get_provider_config', args: {} },
      { command: 'save_provider_config', args: { request: { source: 'git', values: [] } } },
      { command: 'clear_logs', args: {} },
    ])
  })

  it('saves exports through save_export', async () => {
    const invocations = mockInvoke()
    const request = { content: '{}', fileName: 'a.json', format: 'json' } as const

    await setup().saveExport(request)

    expect(invocations).toEqual([{ command: 'save_export', args: { request } }])
  })

  it('turns the string rejection of a command into an Error', async () => {
    mockInvoke(() => {
      throw 'No data yet'
    })

    const failure = setup().call(BackendOperations.activity, undefined)

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
