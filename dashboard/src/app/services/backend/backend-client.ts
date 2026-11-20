import { Service, inject, type ResourceRef } from '@angular/core'
import type {
  CollectAnswer,
  CollectRequest,
  AddConnectionRequestDto,
  AddConnectionResponseDto,
  ConnectionId,
  ExportRequest,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  RecheckAnswer,
  RuntimeState,
} from '../../generated/activity'
import { BackendOperations, type ReadOperation } from './backend-contract'
import { BackendTransport } from './backend-transport'

/** Every backend operation, defined once over whichever transport the runtime picked. */
@Service()
export class ActivityBackendClient {
  private readonly transport = inject(BackendTransport)

  /** A read from `BackendOperations` as a signal resource. */
  readResource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    return this.transport.resource(read)
  }

  getRuntimeState(signal?: AbortSignal): Promise<RuntimeState> {
    return this.transport.call(BackendOperations.runtimeState, undefined, signal)
  }

  collect(request: CollectRequest): Promise<CollectAnswer> {
    return this.transport.call(BackendOperations.collect, request)
  }

  getProviderConfig(): Promise<ProviderConfigSnapshotDto> {
    return this.transport.call(BackendOperations.providerConfig, undefined)
  }

  saveProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto> {
    return this.transport.call(BackendOperations.saveProviderConfig, request)
  }

  addConnection(request: AddConnectionRequestDto): Promise<AddConnectionResponseDto> {
    return this.transport.call(BackendOperations.addConnection, request)
  }

  recheckConnection(connection: ConnectionId): Promise<RecheckAnswer> {
    return this.transport.call(BackendOperations.recheckConnection, connection)
  }

  removeConnection(connection: ConnectionId): Promise<ProviderConfigSaveResponseDto> {
    return this.transport.call(BackendOperations.removeConnection, connection)
  }

  export(request: ExportRequest): Promise<void> {
    return this.transport.saveExport(request)
  }

  /** Opens an http(s) link outside the app, in whichever shell the dashboard runs. */
  openUrl(url: string): Promise<void> {
    return this.transport.openUrl(url)
  }

  /** Awaited so it resolves `undefined`: an empty response arrives as `null` on both transports. */
  async clearLogs(): Promise<void> {
    await this.transport.call(BackendOperations.clearLogs, undefined)
  }
}
