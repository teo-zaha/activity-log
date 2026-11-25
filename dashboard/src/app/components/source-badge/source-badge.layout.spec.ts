import { render } from '@testing-library/angular'
import { expect, it } from 'vitest'
import { SourceBadgeComponent } from './source-badge.component'

it('uses the app theme for the GitHub mark and the Timeline fallback image', async () => {
  const { container } = await render(
    `<app-source-badge source="github" label="GitHub" />
     <span data-src="github">
       <img src="assets/provider-icons/github.svg" data-mono alt="" />
     </span>`,
    { imports: [SourceBadgeComponent] }
  )
  const badge = container.querySelector('app-source-badge')
  const mark = badge?.querySelector('.source-mark')
  const image = container.querySelector('img')
  if (!badge || !mark || !image) throw new Error('both GitHub badge marks must render')
  const html = document.documentElement
  const previousTheme = html.getAttribute('data-theme')
  const wasDark = html.classList.contains('dark')

  try {
    for (const theme of ['light', 'dark']) {
      html.dataset['theme'] = theme
      html.classList.toggle('dark', theme === 'dark')

      const style = getComputedStyle(mark)
      expect(style.maskImage).toContain('assets/provider-icons/github.svg')
      expect(style.backgroundColor).toBe(getComputedStyle(badge).color)
      expect(getComputedStyle(image).filter).toBe(theme === 'dark' ? 'invert(1)' : 'none')
    }
  } finally {
    if (previousTheme === null) html.removeAttribute('data-theme')
    else html.setAttribute('data-theme', previousTheme)
    html.classList.toggle('dark', wasDark)
  }
})
