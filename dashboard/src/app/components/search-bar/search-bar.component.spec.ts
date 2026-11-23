import { ApplicationRef, signal, type WritableSignal } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { SearchBarComponent } from './search-bar.component'

async function setup(initial: string): Promise<WritableSignal<string>> {
  const query = signal(initial)
  await render('<app-search-bar placeholder="Find things" [(value)]="query" />', {
    imports: [SearchBarComponent],
    componentProperties: { query },
  })
  return query
}

describe('SearchBarComponent', () => {
  it('writes what is typed to the bound value and shows outside changes', async () => {
    const query = await setup('abc')
    const input = screen.getByRole('searchbox', { name: 'Find things' })
    expect(input).toHaveProperty('value', 'abc')

    await userEvent.type(input, 'd')
    expect(query()).toBe('abcd')

    query.set('xyz')
    // Rendered by the app's change detection, which the fixture waits for.
    await TestBed.inject(ApplicationRef).whenStable()
    expect(input).toHaveProperty('value', 'xyz')
  })

  it('clears with the button and with Escape', async () => {
    const query = await setup('abc')
    const input = screen.getByRole('searchbox')

    await userEvent.click(screen.getByRole('button', { name: 'Clear search' }))
    expect(query()).toBe('')
    expect(input).toHaveProperty('value', '')
    expect(document.activeElement).toBe(input)

    await userEvent.type(input, 'q')
    expect(query()).toBe('q')
    await userEvent.keyboard('{Escape}')
    expect(query()).toBe('')
  })

  it('takes a separate accessible name and clear label', async () => {
    await render(
      '<app-search-bar placeholder="Filter activity…" label="Filter activity" clearLabel="Clear filter" value="x" />',
      { imports: [SearchBarComponent] }
    )

    expect(screen.getByRole('searchbox', { name: 'Filter activity' })).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Clear filter' })).toBeTruthy()
  })

  it('names the shortcut of its view and tags its key while empty', async () => {
    await render('<app-search-bar placeholder="Filter" shortcut="timeline.filter" />', {
      imports: [SearchBarComponent],
    })
    const input = screen.getByRole('searchbox')

    expect(input.getAttribute('aria-keyshortcuts')).toBe('/')
    expect(document.querySelector('app-key-tag')?.textContent).toBe('/')

    await userEvent.type(input, 'x')
    expect(document.querySelector('app-key-tag')).toBeNull()
  })

  it('has no key without a shortcut', async () => {
    await setup('')

    expect(screen.getByRole('searchbox').hasAttribute('aria-keyshortcuts')).toBe(false)
    expect(document.querySelector('app-key-tag')).toBeNull()
  })
})
