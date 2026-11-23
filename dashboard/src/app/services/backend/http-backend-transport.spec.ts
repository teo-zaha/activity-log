import { provideHttpClient } from '@angular/common/http'
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing'
import { TestBed } from '@angular/core/testing'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { collectingLog, exportRequest } from '../../testing/activity-fixtures'
import { BackendOperations } from './backend-contract'
import { BackendFailure } from './backend-failure'
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

  it('reads the logs over GET /api/logs', async () => {
    const { transport, http } = setup()
    const logs = { entries: [] }

    const result = transport.call(BackendOperations.logs, undefined)
    const req = http.expectOne('/api/logs')
    expect(req.request.method).toBe('GET')
    req.flush(logs)

    await expect(result).resolves.toEqual(logs)
  })

  it('cancels the request itself when the call is aborted', async () => {
    const { transport, http } = setup()
    const controller = new AbortController()

    const result = transport.call(BackendOperations.logs, undefined, controller.signal)
    const req = http.expectOne('/api/logs')
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

  it('rejects desktop-only commands as unsupported without touching the network', async () => {
    const { transport, http } = setup()

    await expect(transport.call(BackendOperations.providerConfig, undefined)).rejects.toMatchObject(
      { kind: 'unsupported' }
    )
    // The browser follows the event stream instead of reading the pair.
    await expect(transport.call(BackendOperations.runtimeState, undefined)).rejects.toMatchObject({
      kind: 'unsupported',
    })
    await expect(
      transport.call(BackendOperations.saveProviderConfig, {
        connection: { source: 'git', name: 'default' },
        values: [],
        basedOn: 'snapshot',
      })
    ).rejects.toBeInstanceOf(BackendFailure)
    http.expectNone(() => true)
  })

  it('rejects a call the backend refused with the kind it answered', async () => {
    const { transport, http } = setup()

    const refused = transport.call(BackendOperations.recheckConnection, {
      source: 'jira',
      name: 'gone',
    })
    http
      .expectOne('/api/connections/recheck')
      .flush(
        { kind: 'notFound', error: 'No source jira/gone' },
        { status: 404, statusText: 'Not Found' }
      )

    await expect(refused).rejects.toMatchObject({
      kind: 'notFound',
      message: 'No source jira/gone',
    })
  })

  it('rejects as unreachable when no answer came at all', async () => {
    const { transport, http } = setup()

    const lost = transport.call(BackendOperations.logs, undefined)
    http.expectOne('/api/logs').error(new ProgressEvent('error'), { status: 0 })

    await expect(lost).rejects.toMatchObject({ kind: 'unreachable' })
  })

  it('never shows a body outside the contract, such as a proxy’s page', async () => {
    const { transport, http } = setup()

    const proxied = transport.call(BackendOperations.logs, undefined)
    http
      .expectOne('/api/logs')
      .flush('<html>Bad Gateway</html>', { status: 502, statusText: 'Bad Gateway' })

    const failure: unknown = await proxied.catch((error: unknown) => error)
    expect(failure).toBeInstanceOf(BackendFailure)
    expect(failure).toMatchObject({ kind: 'internal' })
    expect(String(failure)).not.toContain('<html>')
  })

  it('reads an error body with a kind it does not know as internal', async () => {
    const { transport, http } = setup()

    const odd = transport.call(BackendOperations.logs, undefined)
    http
      .expectOne('/api/logs')
      .flush({ kind: 'later', error: 'from a newer backend' }, { status: 500, statusText: 'Error' })

    await expect(odd).rejects.toMatchObject({ kind: 'internal' })
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

  it('errors desktop-only resources as unsupported', async () => {
    const { transport } = setup()

    const config = TestBed.runInInjectionContext(() =>
      transport.resource(BackendOperations.providerConfig)
    )
    TestBed.tick()

    await vi.waitFor(() => expect(config.error()).toMatchObject({ kind: 'unsupported' }))
  })

  it('errors a read resource with the backend’s failure, as a call does', async () => {
    const { transport, http } = setup()

    const logs = TestBed.runInInjectionContext(() => transport.resource(BackendOperations.logs))
    TestBed.tick()
    http
      .expectOne('/api/logs')
      .flush({ kind: 'internal', error: 'log is gone' }, { status: 500, statusText: 'Error' })

    await vi.waitFor(() => expect(logs.error()).toBeInstanceOf(BackendFailure))
    expect(logs.error()).toMatchObject({ kind: 'internal', message: 'log is gone' })
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
      await expect(saved).resolves.toBe('saved')

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
        { kind: 'invalidRequest', error: 'Unknown time zone "Mars/Olympus"' },
        { status: 400, statusText: 'Bad Request' }
      )

    await expect(saved).rejects.toMatchObject({ kind: 'invalidRequest' })
    expect(click).not.toHaveBeenCalled()
  })

  it('opens web links in a new tab without access to this page', async () => {
    const { transport } = setup()
    const open = vi.spyOn(window, 'open').mockImplementation(() => null)

    await transport.openUrl('https://example.org/a')
    await expect(transport.openUrl('javascript:alert(1)')).rejects.toMatchObject({
      kind: 'invalidRequest',
    })

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

  it('sends nothing for a call aborted before it is sent, and rejects with the reason', async () => {
    const { transport, http } = setup()
    const controller = new AbortController()
    const reason = new Error('stopped')
    controller.abort(reason)

    const call = transport.call(BackendOperations.logs, undefined, controller.signal)

    expect(call).toBeInstanceOf(Promise)
    await expect(call).rejects.toBe(reason)
    http.expectNone('/api/logs')
  })

  it('rejects with a BackendFailure when the browser refuses the download', async () => {
    const { transport, http } = setup()
    const refusal = new DOMException('Blob URLs are off', 'SecurityError')
    vi.stubGlobal('URL', {
      createObjectURL: () => {
        throw refusal
      },
      revokeObjectURL: vi.fn(),
    })

    const saved = transport.saveExport(exportRequest)
    http.expectOne('/api/export').flush({ fileName: 'activity.md', content: '# hi' })

    await expect(saved).rejects.toBeInstanceOf(BackendFailure)
    await expect(saved).rejects.toMatchObject({ kind: 'internal', cause: refusal })
  })

  it('rejects with a BackendFailure when the browser refuses to open a link', async () => {
    const { transport } = setup()
    const refusal = new DOMException('Popups are off', 'SecurityError')
    vi.spyOn(window, 'open').mockImplementation(() => {
      throw refusal
    })

    await expect(transport.openUrl('https://example.org/')).rejects.toMatchObject({
      kind: 'internal',
      cause: refusal,
    })
  })
})
