import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { screen, within } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { confirmAction } from './confirm-dialog.component'

function ask(): Promise<boolean> {
  return confirmAction(TestBed.inject(MatDialog), {
    title: 'Remove GitLab?',
    message: 'Its settings are deleted.',
    confirmLabel: 'Remove',
    tone: 'danger',
  })
}

describe('confirmAction', () => {
  it('resolves true when confirmed', async () => {
    const answer = ask()
    const dialog = await screen.findByRole('alertdialog')
    expect(dialog.textContent).toContain('Its settings are deleted.')

    await userEvent.click(within(dialog).getByRole('button', { name: 'Remove' }))

    expect(await answer).toBe(true)
  })

  it('resolves false when cancelled', async () => {
    const answer = ask()

    await userEvent.click(
      within(await screen.findByRole('alertdialog')).getByRole('button', { name: 'Cancel' })
    )

    expect(await answer).toBe(false)
  })

  it('resolves false when dismissed without an answer', async () => {
    const answer = ask()
    await screen.findByRole('alertdialog')

    TestBed.inject(MatDialog).closeAll()

    expect(await answer).toBe(false)
  })
})
