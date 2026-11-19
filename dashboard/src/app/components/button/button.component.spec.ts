import { render, screen } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import { ButtonComponent } from './button.component'

describe('ButtonComponent', () => {
  it('is a secondary, medium, type="button" button by default', async () => {
    await render(`<button appButton>Copy</button>`, { imports: [ButtonComponent] })

    const button = screen.getByRole('button', { name: 'Copy' })
    expect(button.getAttribute('type')).toBe('button')
    expect(button.getAttribute('data-variant')).toBe('secondary')
    expect(button.getAttribute('data-size')).toBe('md')
    expect(button.hasAttribute('aria-busy')).toBe(false)
  })

  it('keeps an explicit submit type', async () => {
    await render(`<form><button appButton type="submit">Save</button></form>`, {
      imports: [ButtonComponent],
    })

    expect(screen.getByRole('button', { name: 'Save' }).getAttribute('type')).toBe('submit')
  })

  it('reflects the variant and size', async () => {
    await render(`<button appButton variant="danger-ghost" size="lg">Remove</button>`, {
      imports: [ButtonComponent],
    })

    const button = screen.getByRole('button', { name: 'Remove' })
    expect(button.getAttribute('data-variant')).toBe('danger-ghost')
    expect(button.getAttribute('data-size')).toBe('lg')
  })

  it('shows its icon before the label, hidden from the accessible name', async () => {
    await render(`<button appButton icon="refresh">Retry</button>`, { imports: [ButtonComponent] })

    const button = screen.getByRole('button', { name: 'Retry' })
    const icon = button.querySelector('mat-icon')
    expect(icon?.textContent?.trim()).toBe('refresh')
    expect(icon?.getAttribute('aria-hidden')).toBe('true')
    expect(button.firstElementChild).toBe(icon)
  })

  it('marks itself busy and spins in place of its icon without disabling itself', async () => {
    await render(`<button appButton icon="sync" [busy]="true">Collecting…</button>`, {
      imports: [ButtonComponent],
    })

    const button = screen.getByRole<HTMLButtonElement>('button', { name: 'Collecting…' })
    expect(button.getAttribute('aria-busy')).toBe('true')
    expect(button.disabled).toBe(false)
    const icon = button.querySelector('mat-icon')
    expect(icon?.textContent?.trim()).toBe('autorenew')
    expect(icon?.classList.contains('u-spin')).toBe(true)
  })

  it('works on a link, without a type', async () => {
    await render(`<a appButton variant="primary" href="https://example.com">Join</a>`, {
      imports: [ButtonComponent],
    })

    const link = screen.getByRole('link', { name: 'Join' })
    expect(link.hasAttribute('type')).toBe(false)
    expect(link.getAttribute('data-variant')).toBe('primary')
  })
})
