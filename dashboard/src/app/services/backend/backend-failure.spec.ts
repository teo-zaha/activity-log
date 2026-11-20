import { provideHttpClient } from '@angular/common/http'
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing'
import { TestBed } from '@angular/core/testing'
import { mockIPC } from '@tauri-apps/api/mocks'
import { clearTauriMocks } from '../../testing/tauri-mocks'
import { afterEach, describe, expect, it } from 'vitest'
import { BACKEND_ERROR_KINDS, type BackendError } from '../../generated/activity'
import { BackendOperations } from './backend-contract'
import { BackendFailure } from './backend-failure'
import { HttpBackendTransport } from './http-backend-transport'
import { TauriBackendTransport } from './tauri-backend-transport'

/** What a call rejected with, as plain data. */
async function rejection(call: Promise<unknown>): Promise<{ kind: string; message: string }> {
  const error: unknown = await call.then(
    () => null,
    (error: unknown) => error
  )
  if (!(error instanceof BackendFailure))
    throw new Error('the call did not fail as a BackendFailure')
  return { kind: error.kind, message: error.message }
}

/** The failure the HTTP transport gives for the API answering `status` with `body`. */
function overHttp(body: object, status: number): Promise<{ kind: string; message: string }> {
  TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] })
  const call = TestBed.inject(HttpBackendTransport).call(BackendOperations.logs, undefined)
  TestBed.inject(HttpTestingController)
    .expectOne('/api/logs')
    .flush(body, { status, statusText: 'Refused' })
  return rejection(call)
}

/** The failure the Tauri transport gives for a command rejecting with `payload`. */
function overTauri(payload: unknown): Promise<{ kind: string; message: string }> {
  mockIPC(() => {
    throw payload
  })
  return rejection(TestBed.inject(TauriBackendTransport).call(BackendOperations.logs, undefined))
}

describe('BackendFailure', () => {
  afterEach(() => {
    clearTauriMocks()
    TestBed.resetTestingModule()
  })

  it.each(BACKEND_ERROR_KINDS)('is the same over both transports for %s', async kind => {
    const refused: BackendError = { kind, error: `refused as ${kind}` }

    const http = await overHttp(refused, 400)
    TestBed.resetTestingModule()
    const tauri = await overTauri(refused)

    expect(http).toEqual({ kind, message: `refused as ${kind}` })
    expect(tauri).toEqual(http)
  })

  it('is internal over both transports for an answer outside the contract', async () => {
    // Rocket's own error body before it had catchers, and a body with no kind.
    const foreign = { error: { code: 422, reason: 'Unprocessable Entity' } }

    const http = await overHttp(foreign, 422)
    TestBed.resetTestingModule()
    const tauri = await overTauri(foreign)

    expect(http.kind).toBe('internal')
    expect(tauri.kind).toBe('internal')
  })

  it('is unsupported over both transports for an operation the runtime does not have', async () => {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] })
    const http = await rejection(
      TestBed.inject(HttpBackendTransport).call(BackendOperations.providerConfig, undefined)
    )
    const tauri = await rejection(
      TestBed.inject(TauriBackendTransport).call(BackendOperations.renderExport, {
        format: 'json',
        start: '2026-05-17T00:00:00Z',
        end: '2026-05-18T00:00:00Z',
        timeZone: 'Europe/Berlin',
        entries: [],
      })
    )

    expect(http.kind).toBe('unsupported')
    expect(tauri.kind).toBe('unsupported')
  })

  it('rejects with the abort reason over both transports, also for an operation the runtime does not have', async () => {
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] })
    const signal = AbortSignal.abort(new Error('stopped'))
    const http = TestBed.inject(HttpBackendTransport).call(
      BackendOperations.providerConfig,
      undefined,
      signal
    )
    const tauri = TestBed.inject(TauriBackendTransport).call(
      BackendOperations.renderExport,
      {
        format: 'json',
        start: '2026-05-17T00:00:00Z',
        end: '2026-05-18T00:00:00Z',
        timeZone: 'Europe/Berlin',
        entries: [],
      },
      signal
    )

    await expect(http).rejects.toBe(signal.reason)
    await expect(tauri).rejects.toBe(signal.reason)
  })

  it('is an invalid request over both transports for a link that is no web link', async () => {
    mockIPC(() => {
      throw { kind: 'invalidRequest', error: 'Only web links can be opened, not file: links' }
    })
    TestBed.configureTestingModule({ providers: [provideHttpClient(), provideHttpClientTesting()] })

    const http = await rejection(TestBed.inject(HttpBackendTransport).openUrl('file:///etc/passwd'))
    const tauri = await rejection(
      TestBed.inject(TauriBackendTransport).openUrl('file:///etc/passwd')
    )

    expect(http.kind).toBe('invalidRequest')
    expect(tauri.kind).toBe('invalidRequest')
  })
})
