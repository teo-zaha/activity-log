import { provideHttpClient } from '@angular/common/http'
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing'
import { TestBed } from '@angular/core/testing'
import { afterEach, describe, expect, it, vi } from 'vitest'
import {
  activityLog,
  collectingLog,
  exportRequest,
  providersResponse,
} from '../../testing/activity-fixtures'
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

  it('reads the activity over GET /api/activity', async () => {
    const { transport, http } = setup()

    const result = transport.call(BackendOperations.activity, undefined)
    const req = http.expectOne('/api/activity')
    expect(req.request.method).toBe('GET')
    req.flush(activityLog)

    await expect(result).resolves.toEqual(activityLog)
  })

  it('reads the providers over GET /api/providers', async () => {
    const { transport, http } = setup()

    const result = transport.call(BackendOperations.providers, undefined)
    const req = http.expectOne('/api/providers')
    expect(req.request.method).toBe('GET')
    req.flush(providersResponse)

    await expect(result).resolves.toEqual(providersResponse)
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
    const click = vi.spyOn(HTMLAnchorElement.prototype, 'click').mockImplementation(function (
      this: HTMLAnchorElement
    ) {
      downloaded = this.download
    })

    const saved = transport.saveExport(exportRequest)
    const req = http.expectOne('/api/export')
    expect(req.request.method).toBe('POST')
    expect(req.request.body).toEqual(exportRequest)
    req.flush({ fileName: 'activity-2026-05-17-2026-05-18.md', content: '# hi' })
    await saved

    expect(click).toHaveBeenCalledOnce()
    expect(downloaded).toBe('activity-2026-05-17-2026-05-18.md')
    expect(createObjectURL).toHaveBeenCalledWith(expect.objectContaining({ type: 'text/markdown' }))
    expect(revokeObjectURL).toHaveBeenCalledWith('blob:export')
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
