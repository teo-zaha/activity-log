import { Injectable } from '@angular/core'
import { invoke } from '@tauri-apps/api/core'
import type { CollectRequest, ProvidersResponse } from '../../generated/activity'
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

  export(request: ExportRequest): Promise<void> {
    return invoke<void>('save_export', { request })
  }
}
