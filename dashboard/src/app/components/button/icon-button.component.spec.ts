import { render, screen } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import { IconButtonComponent } from './icon-button.component'

describe('IconButtonComponent', () => {
  it('is named by its label and defaults to a ghost, medium, type="button" button', async () => {
    await render(`<button appIconButton icon="close" label="Close details"></button>`, {
      imports: [IconButtonComponent],
    })

    const button = screen.getByRole('button', { name: 'Close details' })
    expect(button.getAttribute('type')).toBe('button')
    expect(button.getAttribute('data-variant')).toBe('ghost')
    expect(button.getAttribute('data-size')).toBe('md')
    expect(button.hasAttribute('data-icon-only')).toBe(true)
    expect(button.querySelector('mat-icon')?.textContent?.trim()).toBe('close')
  })

  it('reflects the variant and size', async () => {
    await render(
      `<button appIconButton icon="description" label="Export" variant="secondary" size="lg"></button>`,
      { imports: [IconButtonComponent] }
    )

    const button = screen.getByRole('button', { name: 'Export' })
    expect(button.getAttribute('data-variant')).toBe('secondary')
    expect(button.getAttribute('data-size')).toBe('lg')
  })

  it('spins while busy and stays enabled', async () => {
    await render(`<button appIconButton icon="sync" label="Collect" [busy]="true"></button>`, {
      imports: [IconButtonComponent],
    })

    const button = screen.getByRole<HTMLButtonElement>('button', { name: 'Collect' })
    expect(button.getAttribute('aria-busy')).toBe('true')
    expect(button.disabled).toBe(false)
    expect(button.querySelector('mat-icon')?.textContent?.trim()).toBe('autorenew')
    expect(button.querySelector('mat-icon')?.classList.contains('u-spin')).toBe(true)
  })

  it('cannot render without a label', async () => {
    // In a compiled template a missing label fails the type check; this is the runtime guard behind it.
    await expect(
      render(`<button appIconButton icon="close"></button>`, { imports: [IconButtonComponent] })
    ).rejects.toThrow(/NG0950/)
  })
})
