import { computed, effect, Service } from '@angular/core'
import { persistedSignal } from '../utils/persisted-signal'

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

@Service()
export class ThemeService {
  readonly theme = persistedSignal<Theme>('theme', 'light')

  readonly isDark = computed(() => DARK_THEMES.includes(this.theme()))

  constructor() {
    this.applyTheme(this.theme())

    effect(() => {
      this.applyTheme(this.theme())
    })
  }

  setTheme(t: Theme): void {
    this.theme.set(t)
  }

  toggle(): void {
    this.theme.update(t => (t === 'light' ? 'dark' : 'light'))
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
    // Keep html.dark in sync so existing :host-context(html.dark) selectors keep working
    el.classList.toggle('dark', isDark)
    el.style.colorScheme = colorScheme
  }
}
