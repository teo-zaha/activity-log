import type { ProviderStatus } from '../../generated/activity'

export function providerStatusLabel(status: ProviderStatus): string {
  switch (status) {
    case 'checking':
      return 'Checking'
    case 'available':
      return 'Available'
    case 'degraded':
      return 'Degraded'
    case 'unavailable':
      return 'Unavailable'
  }
}
