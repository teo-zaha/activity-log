import { Injectable, signal, computed, effect } from '@angular/core'

export type Theme = 'light' | 'green' | 'lilac' | 'solarized' | 'dim' | 'dark' | 'midnight'

export const THEMES: { id: Theme; label: string }[] = [
  { id: 'light',     label: 'Light'     },
  { id: 'green',     label: 'Green'     },
  { id: 'lilac',     label: 'Lilac'     },
  { id: 'solarized', label: 'Solarized' },
  { id: 'dim',       label: 'Dim'       },
  { id: 'dark',      label: 'Dark'      },
  { id: 'midnight',  label: 'Midnight'  },
]

const DARK_THEMES: Theme[] = ['dark', 'dim', 'midnight']

@Injectable({ providedIn: 'root' })
export class ThemeService {
  readonly theme = signal<Theme>((localStorage.getItem('theme') as Theme) ?? 'light')

  readonly isDark = computed(() => DARK_THEMES.includes(this.theme()))

  constructor() {
    this.applyTheme(this.theme())

    effect(() => {
      const t = this.theme()
      this.applyTheme(t)
      localStorage.setItem('theme', t)
    })
  }

  setTheme(t: Theme): void {
    this.theme.set(t)
  }

  toggle(): void {
    this.theme.update(t => t === 'light' ? 'dark' : 'light')
  }

  private applyTheme(t: Theme): void {
    const el = document.documentElement
    el.dataset['theme'] = t
    // Keep html.dark in sync so existing :host-context(html.dark) selectors keep working
    el.classList.toggle('dark', DARK_THEMES.includes(t))
  }
}
