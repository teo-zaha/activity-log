import { render, screen } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import { EmptyStateComponent } from './empty-state.component'

describe('EmptyStateComponent', () => {
  it('renders a real heading at the given level, the message and the action', async () => {
    await render(
      `<app-empty-state heading="No matches" icon="search_off" [level]="2">
        Try another term.
        <button emptyStateAction>Clear filter</button>
      </app-empty-state>`,
      { imports: [EmptyStateComponent] }
    )

    expect(screen.getByRole('heading', { level: 2, name: 'No matches' })).toBeTruthy()
    expect(screen.getByText('Try another term.')).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Clear filter' })).toBeTruthy()
    expect(document.querySelector('.icon-tile mat-icon')?.textContent).toBe('search_off')
  })

  it('defaults to a level-3 heading and lets a visual replace the icon', async () => {
    await render(
      `<app-empty-state heading="Nothing logged yet" icon="inbox" appearance="plain">
        <span emptyStateVisual class="logos">logos</span>
      </app-empty-state>`,
      { imports: [EmptyStateComponent] }
    )

    expect(screen.getByRole('heading', { level: 3, name: 'Nothing logged yet' })).toBeTruthy()
    expect(document.querySelector('.icon-tile')).toBeNull()
    expect(document.querySelector('.logos')).toBeTruthy()
    expect(document.querySelector('app-empty-state')?.getAttribute('data-appearance')).toBe('plain')
  })
})
