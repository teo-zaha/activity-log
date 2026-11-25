import { Component, computed, input, output } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import type { ProviderInfo, ProviderProblem, ProviderStatus } from '../../../../generated/activity'
import { supportedOsLabel } from '../../../../providers/source-catalog'
import { ButtonComponent } from '../../../button/button.component'
import { FeedbackComponent } from '../../../feedback/feedback.component'
import { CredentialRecoveryComponent } from '../../credential-recovery.component'

interface ProviderHealthVm {
  readonly icon: string
  readonly title: string
  readonly reason: string
  /** The startup check has not finished; `checkedAt` is when it started. */
  readonly checking: boolean
}

/** A source's startup check: how it went, when, on which systems, and "Check again". */
@Component({
  selector: 'app-source-health',
  imports: [ButtonComponent, CredentialRecoveryComponent, FeedbackComponent, MatIconModule],
  templateUrl: './source-health.component.html',
  styleUrl: './source-health.component.scss',
})
export class SourceHealthComponent {
  readonly info = input.required<ProviderInfo>()
  /** From the press until the backend has accepted the new check. */
  readonly rechecking = input(false)
  /** Why the last "Check again" was refused. */
  readonly failure = input<string | null>(null)
  readonly recheck = output<void>()

  protected readonly supportedOsLabel = supportedOsLabel
  protected readonly credentialAction = computed(() => {
    const fix = this.info().fixAction
    return fix?.kind === 'web' && fix.action.kind === 'createToken' ? fix.action : undefined
  })

  protected readonly health = computed<ProviderHealthVm>(() => {
    const { status, reason, problem } = this.info()
    const checking = status === 'checking'
    return {
      icon: (problem && PROBLEM_ICONS[problem]) ?? statusIcon(status),
      title: statusTitle(status, problem),
      reason:
        reason ??
        (checking
          ? 'The startup check is still running.'
          : 'No issues were reported during the latest provider inspection.'),
      checking,
    }
  })

  /** Whether a new check can fix what the source reports, as the backend decides it. */
  protected readonly canRecheck = computed(() => this.info().recheckable)

  protected readonly checkedAtLabel = computed(() => {
    const checkedAt = new Date(this.info().checkedAt)
    if (Number.isNaN(checkedAt.getTime())) return 'at startup'
    return checkedAt.toLocaleString('en-GB', {
      day: 'numeric',
      month: 'short',
      hour: '2-digit',
      minute: '2-digit',
    })
  })
}

function statusIcon(status: ProviderStatus): string {
  switch (status) {
    case 'checking':
      return 'hourglass_empty'
    case 'available':
      return 'check_circle'
    case 'degraded':
      return 'warning_amber'
    case 'unavailable':
      return 'block'
  }
}

/** Problems with an icon of their own; the others show their status's icon. */
const PROBLEM_ICONS: Partial<Record<ProviderProblem, string>> = {
  timedOut: 'timer_off',
  refused: 'lock',
  scopeMissing: 'lock',
  throttled: 'speed',
  secretUnavailable: 'key_off',
  unreachable: 'cloud_off',
  notInstalled: 'download',
  notRunning: 'power_settings_new',
  permissionDenied: 'shield',
  noData: 'folder_off',
}

/** What a problem is called, whether the source still collects (degraded) or not. */
const PROBLEM_TITLES: Record<ProviderProblem, string | null> = {
  timedOut: null,
  unsupported: 'Not supported on this system',
  notConfigured: 'Not configured',
  refused: 'Access refused',
  scopeMissing: 'Token permission missing',
  throttled: 'Server busy',
  unreachable: 'Server unreachable',
  notInstalled: 'Not installed',
  notRunning: 'Not running',
  permissionDenied: 'Permission needed',
  noData: 'Nothing to read',
  failed: null,
  collectionFailed: null,
  secretUnavailable: 'Secure store not available',
}

function statusTitle(status: ProviderStatus, problem: ProviderProblem | undefined): string {
  switch (status) {
    case 'checking':
      return 'Checking'
    case 'available':
      return 'Ready to collect'
    case 'degraded':
      if (problem === 'timedOut') return 'Collection timed out'
      if (problem === 'collectionFailed') return 'Collection failed'
      return (problem && PROBLEM_TITLES[problem]) ?? 'Running with issues'
    case 'unavailable':
      if (problem === 'timedOut') return 'Startup check timed out'
      return (problem && PROBLEM_TITLES[problem]) ?? 'Not available'
  }
}
