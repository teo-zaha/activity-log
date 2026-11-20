import { inject, Service } from '@angular/core'
import type {
  AddConnectionRequestDto,
  AddConnectionResponseDto,
  ProviderConfigFieldIssueDto,
  ProviderConfigSaveRequestDto,
  ProviderConfigSaveResponseDto,
  ProviderConfigSnapshotDto,
  RemoveConnectionRequestDto,
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

  // A settings change shows its sources and the collection the backend starts for them: it
  // collects its last range again, so a new source's entries show up by themselves.

  saveProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigSaveResponseDto> {
    return this.data.saveSettings(() => this.backend.saveProviderConfig(request))
  }

  /** What a save of `request` would be refused for; changes nothing. */
  checkProviderConfig(
    request: ProviderConfigSaveRequestDto
  ): Promise<ProviderConfigFieldIssueDto[]> {
    return this.backend.checkProviderConfig(request)
  }

  /** Adds a connection; the providers list then shows it with its startup check. */
  addConnection(request: AddConnectionRequestDto): Promise<AddConnectionResponseDto> {
    return this.data.saveSettings(() => this.backend.addConnection(request))
  }

  removeConnection(request: RemoveConnectionRequestDto): Promise<ProviderConfigSaveResponseDto> {
    return this.data.saveSettings(() => this.backend.removeConnection(request))
  }
}
