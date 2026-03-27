import { HttpClient } from '@angular/common/http'
import { Injectable, inject } from '@angular/core'
import { firstValueFrom } from 'rxjs'
import type { CollectRequest, ProvidersResponse } from '../../generated/activity'
import type { ActivityResponse, ActivityBackendClient } from './backend-client'

@Injectable()
export class HttpBackendClient implements ActivityBackendClient {
  private readonly http = inject(HttpClient)

  getActivity(): Promise<ActivityResponse> {
    return firstValueFrom(this.http.get<ActivityResponse>('/api/activity'))
  }

  getProviders(): Promise<ProvidersResponse> {
    return firstValueFrom(this.http.get<ProvidersResponse>('/api/providers'))
  }

  collect(request: CollectRequest): Promise<ActivityResponse> {
    return firstValueFrom(this.http.post<ActivityResponse>('/api/collect', request))
  }
}
