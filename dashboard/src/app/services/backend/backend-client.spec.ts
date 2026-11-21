import { provideHttpClient } from '@angular/common/http'
import { HttpTestingController, provideHttpClientTesting } from '@angular/common/http/testing'
import type { ResourceRef } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { describe, expect, it, vi } from 'vitest'
import type { ExportOutcome, ExportRequest } from '../../generated/activity'
import { MockBackendTransport, provideMockBackend } from '../../testing/activity-fixtures'
import { ActivityBackendClient } from './backend-client'
import { BackendOperations, type BackendOperation, type ReadOperation } from './backend-contract'
import { BackendTransport } from './backend-transport'
import { HttpBackendTransport } from './http-backend-transport'
import { RuntimeEnvironmentService } from './runtime-environment.service'
import type { RuntimeStream } from './runtime-stream'
import { TauriBackendTransport } from './tauri-backend-transport'

/** Records which concrete transports actually get constructed. */
function configure(tauri: boolean): string[] {
  const constructed: string[] = []
  TestBed.configureTestingModule({
    providers: [
      provideHttpClient(),
      {
        provide: HttpBackendTransport,
        useFactory: (): HttpBackendTransport => {
          constructed.push('http')
          return new HttpBackendTransport()
        },
      },
      {
        provide: TauriBackendTransport,
        useFactory: (): TauriBackendTransport => {
          constructed.push('tauri')
          return new TauriBackendTransport()
        },
      },
      { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => tauri } },
    ],
  })
  return constructed
}

describe('BackendTransport', () => {
  it('resolves to the HTTP transport in the browser, without building the Tauri one', () => {
    const constructed = configure(false)

    expect(TestBed.inject(BackendTransport)).toBeInstanceOf(HttpBackendTransport)
    expect(constructed).toEqual(['http'])
  })

  it('resolves to the Tauri transport in the desktop shell, without building the HTTP one', () => {
    const constructed = configure(true)

    expect(TestBed.inject(BackendTransport)).toBeInstanceOf(TauriBackendTransport)
    expect(constructed).toEqual(['tauri'])
  })

  it('can be replaced by a provider', () => {
    const replacement = new MockBackendTransport()
    TestBed.configureTestingModule({ providers: [provideMockBackend(replacement)] })

    expect(TestBed.inject(BackendTransport)).toBe(replacement)
  })
})

interface RecordedCall {
  readonly operation: BackendOperation<unknown, unknown>
  readonly request: unknown
}

/** Records what the client asks of the transport; none of its promises ever settle. */
class RecordingTransport implements BackendTransport {
  readonly calls: RecordedCall[] = []

  resource<Res>(_read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    throw new Error('resources are not exercised here')
  }

  call<Req, Res>(operation: BackendOperation<Req, Res>, request: NoInfer<Req>): Promise<Res> {
    this.calls.push({ operation, request })
    return new Promise<Res>(() => undefined)
  }

  saveExport(_request: ExportRequest): Promise<ExportOutcome> {
    return new Promise<ExportOutcome>(() => undefined)
  }

  openUrl(_url: string): Promise<void> {
    return new Promise<void>(() => undefined)
  }

  listenForDeepLinks(): Promise<void> {
    return new Promise<void>(() => undefined)
  }

  openRuntimeStream(): RuntimeStream {
    return { close: () => undefined }
  }
}

describe('ActivityBackendClient', () => {
  function setup(): { client: ActivityBackendClient; transport: RecordingTransport } {
    const transport = new RecordingTransport()
    TestBed.configureTestingModule({
      providers: [{ provide: BackendTransport, useValue: transport }],
    })
    return { client: TestBed.inject(ActivityBackendClient), transport }
  }

  it.each([
    ['getProviderConfig', BackendOperations.providerConfig],
    ['clearLogs', BackendOperations.clearLogs],
  ] as const)('%s calls its operation without a request', (method, operation) => {
    const { client, transport } = setup()

    void client[method]()

    expect(transport.calls).toHaveLength(1)
    expect(transport.calls[0].operation).toBe(operation)
    expect(transport.calls[0].request).toBeUndefined()
  })

  it('clearLogs resolves undefined even though the wire answers with null', async () => {
    TestBed.configureTestingModule({
      providers: [provideHttpClient(), provideHttpClientTesting()],
    })
    const client = TestBed.inject(ActivityBackendClient)

    const result = client.clearLogs()
    TestBed.inject(HttpTestingController).expectOne('/api/logs').flush(null)

    await expect(result).resolves.toBeUndefined()
  })

  it('serves reads as resources over the transport', async () => {
    const transport = new MockBackendTransport()
    transport.logs = [
      { id: 1, timestamp: '2026-11-03T08:00:00Z', level: 'info', scope: 'git', message: 'hi' },
    ]
    TestBed.configureTestingModule({ providers: [provideMockBackend(transport)] })
    const client = TestBed.inject(ActivityBackendClient)

    const logs = TestBed.runInInjectionContext(() => client.readResource(BackendOperations.logs))
    await vi.waitFor(() => expect(logs.value()?.entries).toEqual(transport.logs))
  })
})
