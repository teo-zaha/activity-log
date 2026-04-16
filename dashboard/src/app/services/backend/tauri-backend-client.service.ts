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
import type { ExportRequest } from './export-request'

@Injectable()
export class TauriBackendClient implements ActivityBackendClient {
  getActivity(): Promise<ActivityResponse> {
    return invoke<ActivityResponse>('get_activity')
  }

  getProviders(): Promise<ProvidersResponse> {
    return invoke<ProvidersResponse>('get_providers')
  }

  collect(request: CollectRequest): Promise<ActivityResponse> {
    return invoke<ActivityResponse>('collect', { request })
  }

  getProviderConfig(): Promise<ProviderConfigSnapshotDto> {
    return invoke<ProviderConfigSnapshotDto>('get_provider_config')
  }

  saveProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto> {
    return invoke<ProviderConfigSaveResponseDto>('save_provider_config', { request })
  }

  export(request: ExportRequest): Promise<void> {
    return invoke<void>('save_export', { request })
  }
}
