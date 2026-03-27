import { inject, makeEnvironmentProviders } from '@angular/core'
import type {
  ActivityLog,
  CollectRequest,
  CollectingResponse,
  ProvidersResponse,
} from '../../generated/activity'
import { HttpBackendClient } from './http-backend-client.service'
import { RuntimeEnvironmentService } from './runtime-environment.service'
import { TauriBackendClient } from './tauri-backend-client.service'

export type ActivityResponse = ActivityLog | CollectingResponse

export abstract class ActivityBackendClient {
  abstract getActivity(): Promise<ActivityResponse>
  abstract getProviders(): Promise<ProvidersResponse>
  abstract collect(request: CollectRequest): Promise<ActivityResponse>
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
