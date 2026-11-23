import { Service, inject, type ResourceRef } from '@angular/core'
import type {
  CollectAnswer,
  CollectRequest,
  AddConnectionRequestDto,
  AddConnectionResponseDto,
  ConnectionId,
  ExportOutcome,
  ExportRequest,
  LegacyImportReportDto,
  ProviderConfigFieldIssueDto,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  RemoveConnectionRequestDto,
  RecheckAnswer,
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

  checkProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigFieldIssueDto[]> {
    return this.transport.call(BackendOperations.checkProviderConfig, request)
  }

  addConnection(request: AddConnectionRequestDto): Promise<AddConnectionResponseDto> {
    return this.transport.call(BackendOperations.addConnection, request)
  }

  recheckConnection(connection: ConnectionId): Promise<RecheckAnswer> {
    return this.transport.call(BackendOperations.recheckConnection, connection)
  }

  removeConnection(request: RemoveConnectionRequestDto): Promise<ProviderConfigSaveResponseDto> {
    return this.transport.call(BackendOperations.removeConnection, request)
  }

  export(request: ExportRequest): Promise<ExportOutcome> {
    return this.transport.saveExport(request)
  }

  /** What the desktop app's import of 0.0.7's settings did at this start, once; desktop only. */
  takeLegacyImportReport(): Promise<LegacyImportReportDto | null> {
    return this.transport.call(BackendOperations.takeLegacyImportReport, undefined)
  }

  /** Imports 0.0.7's settings now, where that is still due, the keychain allowed to ask. */
  importLegacySettings(): Promise<LegacyImportReportDto | null> {
    return this.transport.call(BackendOperations.importLegacySettings, undefined)
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
