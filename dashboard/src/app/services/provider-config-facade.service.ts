import { inject, Service } from '@angular/core'
import type {
  AddConnectionRequestDto,
  ConnectionId,
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

  saveProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto> {
    return this.changed(this.backend.saveProviderConfig(request))
  }

  /** Adds a connection; the providers list then shows it with its startup check. */
  addConnection(request: AddConnectionRequestDto): Promise<ProviderConfigSaveResponseDto> {
    return this.changed(this.backend.addConnection(request))
  }

  removeConnection(connection: ConnectionId): Promise<ProviderConfigSaveResponseDto> {
    return this.changed(this.backend.removeConnection(connection))
  }

  /**
   * Shows the sources a settings change answers with, and the collection the backend starts for
   * them: it collects its last range again, so a new source's entries show up by themselves.
   */
  private async changed(
    change: Promise<ProviderConfigSaveResponseDto>
  ): Promise<ProviderConfigSaveResponseDto> {
    const response = await change
    this.data.applyProviders(response.providersResponse.providers, true)
    void this.data.followCollection()
    return response
  }
}
