import { render, screen } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import { SourceBadgeComponent } from './source-badge.component'

describe('SourceBadgeComponent', () => {
  it('shows the label and takes its colours from the source', async () => {
    const { container } = await render(
      `<app-source-badge source="gitlab" label="GitLab" size="sm" />`,
      { imports: [SourceBadgeComponent] }
    )

    const badge = container.querySelector('app-source-badge')
    expect(badge?.getAttribute('data-src')).toBe('gitlab')
    expect(badge?.getAttribute('data-size')).toBe('sm')
    expect(screen.getByText('GitLab')).toBeTruthy()
  })

  it('keeps the label for screen readers when it shows only the icon on phones', async () => {
    const { container } = await render(
      `<app-source-badge source="git" label="Git" iconOnlyOnPhone />`,
      { imports: [SourceBadgeComponent] }
    )

    expect(container.querySelector('app-source-badge')?.classList).toContain('icon-only-on-phone')
    expect(screen.getByText('Git')).toBeTruthy()
  })

  it('shows a second source after the first, in its own colours', async () => {
    await render(
      `<app-source-badge source="git" label="Git" [split]="{ source: 'gitlab', label: 'Lab' }" />`,
      { imports: [SourceBadgeComponent] }
    )

    expect(screen.getByText('Git').closest('[data-src]')?.getAttribute('data-src')).toBe('git')
    expect(screen.getByText('Lab').getAttribute('data-src')).toBe('gitlab')
  })
})
