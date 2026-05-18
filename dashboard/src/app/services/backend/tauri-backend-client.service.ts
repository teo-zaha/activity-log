import { Injectable } from '@angular/core'
import { invoke } from '@tauri-apps/api/core'
import type {
  CollectRequest,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  ProvidersResponse,
} from '../../generated/activity'
import type { ActivityResponse, ActivityBackendClient } from './backend-client'
import { TauriCommands } from './backend-contract'
import type { ExportRequest } from './export-request'

@Injectable()
export class TauriBackendClient implements ActivityBackendClient {
  getActivity(): Promise<ActivityResponse> {
    return invoke<ActivityResponse>(TauriCommands.getActivity)
  }

  getProviders(): Promise<ProvidersResponse> {
    return invoke<ProvidersResponse>(TauriCommands.getProviders)
  }

  collect(request: CollectRequest): Promise<ActivityResponse> {
    return invoke<ActivityResponse>(TauriCommands.collect, { request })
  }

  getProviderConfig(): Promise<ProviderConfigSnapshotDto> {
    return invoke<ProviderConfigSnapshotDto>(TauriCommands.getProviderConfig)
  }

  saveProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto> {
    return invoke<ProviderConfigSaveResponseDto>(TauriCommands.saveProviderConfig, { request })
  }

  export(request: ExportRequest): Promise<void> {
    return invoke<void>(TauriCommands.saveExport, { request })
  }
}
