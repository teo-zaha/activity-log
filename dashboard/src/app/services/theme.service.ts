import { computed, effect, Service } from '@angular/core'
import { mediaQuery } from '@signality/core/browser/media-query'
import { storage } from '@signality/core/browser/storage'

export type Theme =
  'light' | 'green' | 'lilac' | 'solarized' | 'dim' | 'dark' | 'midnight' | 'mocha'

export const THEMES: { id: Theme; label: string }[] = [
  { id: 'light', label: 'Light' },
  { id: 'green', label: 'Green' },
  { id: 'lilac', label: 'Lilac' },
  { id: 'solarized', label: 'Solarized' },
  { id: 'dim', label: 'Dim' },
  { id: 'mocha', label: 'Mocha' },
  { id: 'dark', label: 'Dark' },
  { id: 'midnight', label: 'Midnight' },
]

const DARK_THEMES: Theme[] = ['dark', 'dim', 'midnight', 'mocha']

/** What the device prefers, for as long as no theme has been chosen. */
const DEVICE_DARK_QUERY = '(prefers-color-scheme: dark)'

@Service()
export class ThemeService {
  /**
   * The theme picked in the settings; `null` until one is, and then nothing is stored. An
   * unknown stored theme, say from an older version, counts as none.
   */
  private readonly chosen = storage<Theme | null>('theme', null, {
    serializer: {
      read: stored => THEMES.find(option => option.id === stored)?.id ?? null,
      write: theme => theme ?? '',
    },
  })
  private readonly deviceDark = mediaQuery(DEVICE_DARK_QUERY)

  /** The chosen theme, else Dark or Light as the device is, following it while it changes. */
  readonly theme = computed<Theme>(() => this.chosen() ?? (this.deviceDark() ? 'dark' : 'light'))

  readonly isDark = computed(() => DARK_THEMES.includes(this.theme()))

  constructor() {
    this.applyTheme(this.theme())

    effect(() => {
      this.applyTheme(this.theme())
    })
  }

  setTheme(t: Theme): void {
    this.chosen.set(t)
  }

  private applyTheme(t: Theme): void {
    const el = document.documentElement
    const isDark = DARK_THEMES.includes(t)
    const colorScheme = isDark ? 'dark' : 'light'

    if (
      el.dataset['theme'] === t &&
      el.classList.contains('dark') === isDark &&
      el.style.colorScheme === colorScheme
    ) {
      return
    }

    el.dataset['theme'] = t
    // html.dark marks the dark themes for styles.scss and components' :host-context(html.dark)
    el.classList.toggle('dark', isDark)
    el.style.colorScheme = colorScheme
  }
}
