import { inject, makeEnvironmentProviders } from '@angular/core'
import type {
  ActivityLog,
  CollectRequest,
  CollectingResponse,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  ProvidersResponse,
} from '../../generated/activity'
import type { ExportRequest } from './export-request'
import { HttpBackendClient } from './http-backend-client.service'
import { RuntimeEnvironmentService } from './runtime-environment.service'
import { TauriBackendClient } from './tauri-backend-client.service'

export type ActivityResponse = ActivityLog | CollectingResponse

export class DesktopOnlyBackendError extends Error {
  constructor(operation: string) {
    super(`${operation} is only available in desktop mode.`)
    this.name = 'DesktopOnlyBackendError'
  }
}

export abstract class ActivityBackendClient {
  abstract getActivity(): Promise<ActivityResponse>
  abstract getProviders(): Promise<ProvidersResponse>
  abstract collect(request: CollectRequest): Promise<ActivityResponse>
  abstract getProviderConfig(): Promise<ProviderConfigSnapshotDto>
  abstract saveProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto>
  abstract export(request: ExportRequest): Promise<void>
}

export function provideActivityBackendClient() {
  return makeEnvironmentProviders([
    HttpBackendClient,
    TauriBackendClient,
    {
      provide: ActivityBackendClient,
      useFactory: () => {
        const environment = inject(RuntimeEnvironmentService)
        return environment.isTauri() ? inject(TauriBackendClient) : inject(HttpBackendClient)
      },
    },
  ])
}
