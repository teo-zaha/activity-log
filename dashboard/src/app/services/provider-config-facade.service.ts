import { inject, Service } from '@angular/core'
import type {
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
} from '../generated/activity'
import { ActivityBackendClient } from './backend/backend-client'
import { ActivityDataService } from './activity-data.service'

@Service()
export class ProviderConfigFacade {
  private readonly backend = inject(ActivityBackendClient)
  private readonly data = inject(ActivityDataService)

  loadProviderConfig(): Promise<ProviderConfigSnapshotDto> {
    return this.backend.getProviderConfig()
  }

  async saveProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto> {
    const response = await this.backend.saveProviderConfig(request)
    this.data.applyProviders(response.providersResponse.providers, true)
    return response
  }
}
