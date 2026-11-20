import { provideHttpClient } from '@angular/common/http'
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing'
import { TestBed } from '@angular/core/testing'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { collectingLog, exportRequest, MockBackendTransport } from '../../testing/activity-fixtures'
import { BackendOperations, DesktopOnlyBackendError } from './backend-contract'
import { HttpBackendTransport } from './http-backend-transport'

function setup(): { transport: HttpBackendTransport; http: HttpTestingController } {
  TestBed.configureTestingModule({
    providers: [provideHttpClient(), provideHttpClientTesting()],
  })
  return {
    transport: TestBed.inject(HttpBackendTransport),
    http: TestBed.inject(HttpTestingController),
  }
}

describe('HttpBackendTransport', () => {
  afterEach(() => {
    vi.unstubAllGlobals()
    vi.restoreAllMocks()
    TestBed.inject(HttpTestingController).verify()
  })

  it('reads the host’s state over GET /api/runtime-state', async () => {
    const { transport, http } = setup()
    const state = new MockBackendTransport().runtimeState()

    const result = transport.call(BackendOperations.runtimeState, undefined)
    const req = http.expectOne('/api/runtime-state')
    expect(req.request.method).toBe('GET')
    req.flush(state)

    await expect(result).resolves.toEqual(state)
  })

  it('cancels the request itself when the call is aborted', async () => {
    const { transport, http } = setup()
    const controller = new AbortController()

    const result = transport.call(BackendOperations.runtimeState, undefined, controller.signal)
    const req = http.expectOne('/api/runtime-state')
    controller.abort(new Error('stopped'))

    await expect(result).rejects.toThrow('stopped')
    expect(req.cancelled).toBe(true)
  })

  it('collects over POST /api/collect with the request as body', async () => {
    const { transport, http } = setup()
    const request = { start: '2026-05-17T00:00:00Z', end: '2026-05-19T00:00:00Z' }

    const result = transport.call(BackendOperations.collect, request)
    const req = http.expectOne('/api/collect')
    expect(req.request.method).toBe('POST')
    expect(req.request.body).toEqual(request)
    req.flush(collectingLog)

    await expect(result).resolves.toEqual(collectingLog)
  })

  it('clears logs over DELETE /api/logs', async () => {
    const { transport, http } = setup()

    const result = transport.call(BackendOperations.clearLogs, undefined)
    const req = http.expectOne('/api/logs')
    expect(req.request.method).toBe('DELETE')
    req.flush(null)

    await result
  })

  it('rejects desktop-only commands without touching the network', async () => {
    const { transport, http } = setup()

    await expect(
      transport.call(BackendOperations.providerConfig, undefined)
    ).rejects.toBeInstanceOf(DesktopOnlyBackendError)
    await expect(
      transport.call(BackendOperations.saveProviderConfig, {
        connection: { source: 'git', name: 'default' },
        values: [],
      })
    ).rejects.toThrow('Provider configuration is only available in desktop mode.')
    http.expectNone(() => true)
  })

  it('loads reads as GET resources', async () => {
    const { transport, http } = setup()

    const logs = TestBed.runInInjectionContext(() => transport.resource(BackendOperations.logs))
    TestBed.tick()
    const req = http.expectOne('/api/logs')
    expect(req.request.method).toBe('GET')
    req.flush({ entries: [] })

    await vi.waitFor(() => expect(logs.value()).toEqual({ entries: [] }))
  })

  it('errors desktop-only resources with DesktopOnlyBackendError', async () => {
    const { transport } = setup()

    const config = TestBed.runInInjectionContext(() =>
      transport.resource(BackendOperations.providerConfig)
    )
    TestBed.tick()

    await vi.waitFor(() => expect(config.error()).toBeInstanceOf(DesktopOnlyBackendError))
  })

  it('has the API render an export and downloads it through a temporary object URL', async () => {
    const { transport, http } = setup()
    const createObjectURL = vi.fn(() => 'blob:export')
    const revokeObjectURL = vi.fn()
    vi.stubGlobal('URL', { createObjectURL, revokeObjectURL })
    let downloaded = ''
    let attached = false
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement
    ) {
      downloaded = this.download
      attached = this.isConnected
    })
    vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] })

    try {
      const saved = transport.saveExport(exportRequest)
      const req = http.expectOne('/api/export')
      expect(req.request.method).toBe('POST')
      expect(req.request.body).toEqual(exportRequest)
      req.flush({ fileName: 'activity-2026-05-17-2026-05-18.md', content: '# hi' })
      await saved

      expect(click).toHaveBeenCalledOnce()
      expect(downloaded).toBe('activity-2026-05-17-2026-05-18.md')
      // Firefox follows a click only on a link in the document; it goes again afterwards.
      expect(attached).toBe(true)
      expect(document.querySelector('a[download]')).toBeNull()
      expect(createObjectURL).toHaveBeenCalledWith(
        expect.objectContaining({ type: 'text/markdown' })
      )
      // The browser reads the file after the click returns: the URL stays valid until then.
      expect(revokeObjectURL).not.toHaveBeenCalled()
      vi.runAllTimers()
      expect(revokeObjectURL).toHaveBeenCalledWith('blob:export')
    } finally {
      vi.useRealTimers()
    }
  })

  it('downloads nothing when the API rejects an export', async () => {
    const { transport, http } = setup()
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(() => undefined)

    const saved = transport.saveExport({ ...exportRequest, timeZone: 'Mars/Olympus' })
    http
      .expectOne('/api/export')
      .flush(
        { error: 'Unknown time zone "Mars/Olympus"' },
        { status: 400, statusText: 'Bad Request' }
      )

    await expect(saved).rejects.toMatchObject({ status: 400 })
    expect(click).not.toHaveBeenCalled()
  })

  it('opens web links in a new tab without access to this page', async () => {
    const { transport } = setup()
    const open = vi.spyOn(window, 'open').mockImplementation(() => null)

    await transport.openUrl('https://example.org/a')
    await expect(transport.openUrl('javascript:alert(1)')).rejects.toThrow('Only web links')

    expect(open).toHaveBeenCalledExactlyOnceWith(
      'https://example.org/a',
      '_blank',
      'noopener,noreferrer'
    )
  })

  it('refuses a request that does not match the operation', () => {
    const { transport } = setup()

    // Type-level only: never invoked, but still checked by the compiler.
    const mismatches = (): void => {
      // @ts-expect-error a collect request is required
      void transport.call(BackendOperations.collect, undefined)
      // @ts-expect-error the save request is required
      void transport.call(BackendOperations.saveProviderConfig, null)
      // @ts-expect-error reads take no request
      void transport.call(BackendOperations.activity, { start: 'x' })
    }

    expect(mismatches).toBeTypeOf('function')
  })
})
