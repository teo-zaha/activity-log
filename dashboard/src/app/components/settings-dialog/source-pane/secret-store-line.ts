import type { SecretStoreInfo } from '../../../generated/activity'
import type { FeedbackTone } from '../../feedback/feedback.component'

/** What the settings say about where new secrets are sealed; nothing for the usual case. */
export interface SecretStoreLine {
  readonly text: string
  readonly tone: FeedbackTone
}

/** The line for `info`: none while the OS store answers, one for every other state. */
export function secretStoreLine(info: SecretStoreInfo): SecretStoreLine | null {
  switch (info.store) {
    case 'os':
      switch (info.state) {
        case 'ready':
          return null
        case 'refused':
          return {
            text: 'The keychain does not answer now: saving a token waits until it does.',
            tone: 'warn',
          }
      }
    case 'file':
      switch (info.why) {
        case 'noOsStore':
          return {
            text: 'Secrets are protected by a key file on this computer (no keychain available).',
            tone: 'info',
          }
        case 'forced':
          return {
            text: 'Secrets are protected by a key file, as ACTIVITY_LOG_SECRET_STORE asks.',
            tone: 'info',
          }
        case 'devProfile':
          return {
            text: "Development build: secrets are kept in the dev profile's key file.",
            tone: 'info',
          }
      }
    case 'unavailable':
      switch (info.why) {
        case 'noOsStore':
          return {
            text: 'This computer has no keychain, so tokens cannot be saved here. Install or start a keyring service (GNOME Keyring, KWallet, KeePassXC).',
            tone: 'warn',
          }
      }
  }
}
