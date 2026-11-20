import { Component, computed, inject, output, resource, signal } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import { FeedbackComponent } from '../../feedback/feedback.component'
import { ButtonComponent } from '../../button/button.component'
import type { AddConnectionRequestDto, ProviderKindDto } from '../../../generated/activity'
import { type ConnectionKey, connectionKey } from '../../../providers/connection'
import { UPCOMING_SOURCES, onlyOnLabel } from '../../../providers/source-catalog'
import { ActivityService } from '../../../services/activity.service'
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
  private readonly activity = inject(ActivityService)
  private readonly runtime = inject(RuntimeEnvironmentService)

  /** Emitted with the connection to open: the one just added, or the one a tile stands for. */
  readonly opened = output<ConnectionKey>()

  protected readonly isDesktop = this.runtime.isTauri()
  protected readonly upcoming = UPCOMING_SOURCES

  private readonly config = resource({
    params: () => (this.isDesktop ? true : undefined),
    loader: () => this.activity.loadProviderConfig(),
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

  /** The tile being added, so it can show progress while the others wait. */
  protected readonly adding = signal<string | null>(null)
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
        void this.add({ source: tile.kind.source }, tile.kind.source)
    }
  }

  protected chooseLocation(location: GitLocation): void {
    const request: AddConnectionRequestDto =
      location.wslDistro === null
        ? { source: 'git' }
        : { source: 'git', wslDistro: location.wslDistro }
    void this.add(request, location.label)
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

  private async add(request: AddConnectionRequestDto, tileId: string): Promise<void> {
    if (this.adding() !== null || !this.config.hasValue()) return

    this.adding.set(tileId)
    this.error.set(null)
    try {
      const response = await this.activity.addConnection(request)
      this.config.set(response.snapshot)
      this.opened.emit(connectionKey(response.created))
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
