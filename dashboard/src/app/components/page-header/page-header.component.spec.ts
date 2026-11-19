import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { PageHeaderComponent } from './page-header.component'

describe('PageHeaderComponent', () => {
  it('shows a back arrow and a close button only when they are named', async () => {
    const goBack = vi.fn()
    const dismiss = vi.fn()
    const labels: { back: string | null; close: string | null } = {
      back: 'Back to settings',
      close: null,
    }
    const { rerender } = await render(
      `<app-page-header [backLabel]="back" [closeLabel]="close" (goBack)="goBack()" (dismiss)="dismiss()">
        <h2>Logs</h2>
        <button pageAction>Copy</button>
      </app-page-header>`,
      {
        imports: [PageHeaderComponent],
        componentProperties: { ...labels, goBack, dismiss },
      }
    )

    expect(screen.getByRole('heading', { name: 'Logs' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Copy' })).toBeTruthy()
    expect(screen.queryByRole('button', { name: 'Close settings' })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Back to settings' }))
    expect(goBack).toHaveBeenCalledOnce()

    await rerender({
      componentProperties: { back: null, close: 'Close settings', goBack, dismiss },
    })
    expect(screen.queryByRole('button', { name: 'Back to settings' })).toBeNull()
    await userEvent.click(screen.getByRole('button', { name: 'Close settings' }))
    expect(dismiss).toHaveBeenCalledOnce()
  })
})
