import { Service } from '@angular/core'
import { storage } from '@signality/core/browser/storage'
import type { CharacterKeys } from '../models/shortcuts'

/**
 * How the keyboard is used on this device, kept like the theme: whether single-key shortcuts
 * work (WCAG 2.1.4). Other tabs follow a change. Not in the URL, since a link must not change
 * someone's keys, and not in `config.toml`, the connections file.
 */
@Service()
export class ShortcutPreferences {
  /** `off` once turned off; anything else stored, or nothing, reads as the default, `on`. */
  private readonly stored = storage<CharacterKeys>('character-shortcuts', 'on', {
    serializer: {
      read: stored => (stored === 'off' ? 'off' : 'on'),
      write: value => value,
    },
  })

  readonly characterKeys = this.stored.asReadonly()

  setCharacterKeys(value: CharacterKeys): void {
    this.stored.set(value)
  }
}
