import { Component, computed, inject, output, resource, signal } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { FeedbackComponent } from '../../feedback/feedback.component'
import { ButtonComponent } from '../../button/button.component'
import type { AddConnectionRequestDto, ProviderKindDto } from '../../../generated/activity'
import { type ConnectionKey, connectionKey } from '../../../providers/connection'
import { UPCOMING_SOURCES, onlyOnLabel } from '../../../providers/source-catalog'
import { ActivityDataService } from '../../../services/activity-data.service'
import { ConnectionSettings } from '../../../services/connection-settings.service'
import { readOnHost } from '../../../services/read-on-host'
import { type Failure, failureOf, refusedAs } from '../../../services/failure'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
import { ProviderIconComponent } from '../../provider-icon/provider-icon.component'

/** What clicking a kind's tile does. */
type TileState =
  | { readonly kind: 'add'; readonly count: number }
  /** A kind with a single connection that is already set up: the tile opens it. */
  | { readonly kind: 'added'; readonly key: ConnectionKey }
  | { readonly kind: 'unsupported'; readonly reason: string }

interface SourceTile {
  readonly kind: ProviderKindDto
  readonly description: string
  readonly state: TileState
}

/** Where a new Git connection reads: this computer, or a WSL distribution. */
interface GitLocation {
  readonly wslDistro: string | null
  readonly label: string
  readonly description: string
  readonly icon: string
}

@Component({
  selector: 'app-settings-add-source-pane',
  imports: [FeedbackComponent, ButtonComponent, MatIconModule, ProviderIconComponent],
  templateUrl: './settings-add-source-pane.component.html',
  styleUrl: './settings-add-source-pane.component.scss',
})
export class SettingsAddSourcePaneComponent {
  private readonly data = inject(ActivityDataService)
  private readonly settings = inject(ConnectionSettings)
  private readonly runtime = inject(RuntimeEnvironmentService)

  /** Emitted with the connection to open: the one just added, or the one a tile stands for. */
  readonly opened = output<ConnectionKey>()

  protected readonly isDesktop = this.runtime.isTauri()
  protected readonly upcoming = UPCOMING_SOURCES

  /** The snapshot of the host on screen: a read the host restarted under is sent again. */
  private readonly config = resource({
    params: () => (this.isDesktop ? true : undefined),
    loader: () => readOnHost(this.data.hostEpoch, () => this.settings.loadProviderConfig()),
  })
  protected readonly loading = this.config.isLoading
  protected readonly loadError = computed(() => {
    const error = this.config.error()
    return error ? failureOf(error).message : null
  })

  protected readonly tiles = computed<SourceTile[]>(() => {
    if (!this.config.hasValue()) return []
    const { kinds, connections } = this.config.value()
    return kinds.map(kind => {
      const existing = connections.filter(state => state.connection.source === kind.source)
      return {
        kind,
        description: kind.description,
        state: this.tileState(
          kind,
          existing.map(state => connectionKey(state.connection))
        ),
      }
    })
  })

  /** Set while Git asks where to read, which only happens with WSL distributions to offer. */
  protected readonly locationsFor = signal<ProviderKindDto | null>(null)
  protected readonly locations = computed<GitLocation[]>(() => {
    const kind = this.locationsFor()
    if (!kind) return []
    return [
      {
        wslDistro: null,
        label: 'This computer',
        description: 'Repositories in folders on this computer',
        icon: 'computer',
      },
      ...kind.wslDistros.map(distro => ({
        wslDistro: distro,
        label: `WSL · ${distro}`,
        description: 'Repositories in the home folder of this distribution',
        icon: 'terminal',
      })),
    ]
  })

  /** The connection being added, so its tile can show progress while the others wait. */
  private readonly adding = signal<AddConnectionRequestDto | null>(null)
  protected readonly busy = computed(() => this.adding() !== null)
  /** The kind whose tile shows progress. */
  protected readonly addingKind = computed(() => this.adding()?.source ?? null)
  /** The Git location whose tile shows progress: its distribution, `null` for this computer. */
  protected readonly addingLocation = computed(() => {
    const request = this.adding()
    return request?.source === 'git' ? { wslDistro: request.wslDistro ?? null } : null
  })
  protected readonly error = signal<Failure | null>(null)

  protected choose(tile: SourceTile): void {
    switch (tile.state.kind) {
      case 'added':
        this.opened.emit(tile.state.key)
        return
      case 'unsupported':
        return
      case 'add':
        if (tile.kind.wslDistros.length > 0) {
          this.error.set(null)
          this.locationsFor.set(tile.kind)
          return
        }
        void this.add({ source: tile.kind.source })
    }
  }

  protected chooseLocation(location: GitLocation): void {
    const request: AddConnectionRequestDto =
      location.wslDistro === null
        ? { source: 'git' }
        : { source: 'git', wslDistro: location.wslDistro }
    void this.add(request)
  }

  protected backToSources(): void {
    this.locationsFor.set(null)
    this.error.set(null)
  }

  protected stateLabel(state: TileState): string {
    switch (state.kind) {
      case 'add':
        return state.count === 0 ? 'Add' : `${state.count} added · Add another`
      case 'added':
        return 'Added · Open'
      case 'unsupported':
        return state.reason
    }
  }

  protected stateIcon(state: TileState): string {
    switch (state.kind) {
      case 'add':
        return 'add'
      case 'added':
        return 'check'
      case 'unsupported':
        return 'block'
    }
  }

  private tileState(kind: ProviderKindDto, existing: ConnectionKey[]): TileState {
    if (!kind.supported) return { kind: 'unsupported', reason: onlyOnLabel(kind.supportedOs) }
    const [first] = existing
    if (!kind.multiple && first) return { kind: 'added', key: first }
    return { kind: 'add', count: existing.length }
  }

  private async add(request: AddConnectionRequestDto): Promise<void> {
    if (this.adding() !== null || !this.config.hasValue()) return

    this.adding.set(request)
    this.error.set(null)
    try {
      const response = await this.settings.addConnection(request)
      const standing = this.data.receiptStanding(response.installed)
      // The answer's snapshot shows only while nothing later is on screen; otherwise it is read
      // again. The new source opens only if the host on screen installed it: one that another
      // host answered, or that was written but never installed, would open as not found.
      if (standing === 'current') this.config.set(response.snapshot)
      else this.config.reload()
      if (standing === 'current' || standing === 'superseded') {
        this.opened.emit(connectionKey(response.created))
      }
    } catch (error) {
      const failed = failureOf(error, 'Failed to add the source.')
      this.error.set(failed)
      // Another program added it meanwhile: the snapshot shows it added, to open.
      if (refusedAs(failed, 'alreadyExists')) this.config.reload()
    } finally {
      this.adding.set(null)
    }
  }
}
