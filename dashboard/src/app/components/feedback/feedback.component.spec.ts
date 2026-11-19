import { render, screen } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import { FeedbackComponent } from './feedback.component'

describe('FeedbackComponent', () => {
  it('announces an error as an alert with the error icon', async () => {
    await render(`<app-feedback tone="error">Could not load</app-feedback>`, {
      imports: [FeedbackComponent],
    })

    const alert = screen.getByRole('alert')
    expect(alert.textContent).toContain('Could not load')
    expect(alert.getAttribute('data-tone')).toBe('error')
    expect(alert.querySelector('mat-icon')?.textContent).toBe('error_outline')
  })

  it.each([
    ['info', 'info'],
    ['ok', 'check_circle'],
    ['warn', 'warning_amber'],
  ] as const)('announces %s politely with its icon', async (tone, icon) => {
    await render(`<app-feedback [tone]="tone">Saved</app-feedback>`, {
      imports: [FeedbackComponent],
      componentProperties: { tone },
    })

    const status = screen.getByRole('status')
    expect(status.querySelector('mat-icon')?.textContent).toBe(icon)
    expect(screen.queryByRole('alert')).toBeNull()
  })

  it('puts an action at the end of a block message', async () => {
    await render(
      `<app-feedback tone="error" block>Failed <button feedbackAction>Retry</button></app-feedback>`,
      { imports: [FeedbackComponent] }
    )

    const alert = screen.getByRole('alert')
    expect(alert.classList).toContain('is-block')
    expect(alert.lastElementChild).toBe(screen.getByRole('button', { name: 'Retry' }))
  })
})
