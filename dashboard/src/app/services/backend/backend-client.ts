import { Service, inject, type ResourceRef } from '@angular/core'
import type {
  CollectRequest,
  AddConnectionRequestDto,
  ConnectionId,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  ProvidersResponse,
} from '../../generated/activity'
import { type ActivityResponse, BackendOperations, type ReadOperation } from './backend-contract'
import { BackendTransport } from './backend-transport'
import type { ExportRequest } from './export-request'

/** Every backend operation, defined once over whichever transport the runtime picked. */
@Service()
export class ActivityBackendClient {
  private readonly transport = inject(BackendTransport)

  /** A read from `BackendOperations` as a signal resource. */
  readResource<Res>(read: ReadOperation<Res>): ResourceRef<Res | undefined> {
    return this.transport.resource(read)
  }

  getActivity(): Promise<ActivityResponse> {
    return this.transport.call(BackendOperations.activity, undefined)
  }

  getProviders(): Promise<ProvidersResponse> {
    return this.transport.call(BackendOperations.providers, undefined)
  }

  collect(request: CollectRequest): Promise<ActivityResponse> {
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

  addConnection(request: AddConnectionRequestDto): Promise<ProviderConfigSaveResponseDto> {
    return this.transport.call(BackendOperations.addConnection, request)
  }

  removeConnection(connection: ConnectionId): Promise<ProviderConfigSaveResponseDto> {
    return this.transport.call(BackendOperations.removeConnection, connection)
  }

  export(request: ExportRequest): Promise<void> {
    return this.transport.saveExport(request)
  }

  /** Awaited so it resolves `undefined`: an empty response arrives as `null` on both transports. */
  async clearLogs(): Promise<void> {
    await this.transport.call(BackendOperations.clearLogs, undefined)
  }
}
