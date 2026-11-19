import { Component, computed, input } from '@angular/core'
import { MatIconModule } from '@angular/material/icon'
import type { Source } from '../../generated/activity'
import { getProviderAdapter } from '../../providers/provider-adapter'

const BRAND_ICONS: Partial<Record<Source, string>> = {
  git: 'assets/provider-icons/git.svg',
  gitlab: 'assets/provider-icons/gitlab.svg',
  jira: 'assets/provider-icons/jira.svg',
  outlook: 'assets/provider-icons/outlook.svg',
}

/** The logo of `source`, or null when it shows a Material icon instead. */
export function sourceLogoUrl(source: Source): string | null {
  return BRAND_ICONS[source] ?? null
}

/**
 * A source's logo, or its Material icon when it has none. With `size`, it sits in a rounded box
 * in the source's colours; the box takes the colours of an enclosing `[data-src]` when it has no
 * source of its own.
 */
@Component({
  selector: 'app-provider-icon',
  imports: [MatIconModule],
  templateUrl: './provider-icon.component.html',
  styleUrl: './provider-icon.component.scss',
  host: {
    '[attr.data-src]': 'source()',
    '[attr.data-size]': 'size()',
  },
})
export class ProviderIconComponent {
  readonly source = input<Source | null>(null)
  /** Shown when there is no logo; defaults to the source's icon. */
  readonly fallbackIcon = input<string | null>(null)
  /** A logo that is not a source's, such as one of an upcoming source. */
  readonly iconUrl = input<string | null>(null)
  readonly size = input<'sm' | 'md' | 'lg' | null>(null)

  protected readonly logoUrl = computed(() => {
    const source = this.source()
    return this.iconUrl() ?? (source ? sourceLogoUrl(source) : null)
  })
  protected readonly icon = computed(() => {
    const source = this.source()
    return this.fallbackIcon() ?? (source ? getProviderAdapter(source).providerIcon : 'help')
  })
}
