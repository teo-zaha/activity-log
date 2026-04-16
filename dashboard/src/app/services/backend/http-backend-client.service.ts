import { HttpClient } from '@angular/common/http'
import { Injectable, inject } from '@angular/core'
import { firstValueFrom } from 'rxjs'
import type {
  CollectRequest,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  ProvidersResponse,
} from '../../generated/activity'
import {
  ActivityBackendClient,
  type ActivityResponse,
  DesktopOnlyBackendError,
} from './backend-client'
import type { ExportRequest } from './export-request'

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

  getProviderConfig(): Promise<ProviderConfigSnapshotDto> {
    return Promise.reject(new DesktopOnlyBackendError('Provider configuration'))
  }

  saveProviderConfig(
    _request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto> {
    return Promise.reject(new DesktopOnlyBackendError('Provider configuration'))
  }

  async export(request: ExportRequest): Promise<void> {
    const blob = new Blob([request.content], { type: this.mimeTypeFor(request.format) })
    const href = URL.createObjectURL(blob)

    try {
      const link = document.createElement('a')
      link.href = href
      link.download = request.fileName
      link.click()
    } finally {
      URL.revokeObjectURL(href)
    }
  }

  private mimeTypeFor(format: ExportRequest['format']): string {
    return format === 'markdown' ? 'text/markdown' : 'application/json'
  }
}
