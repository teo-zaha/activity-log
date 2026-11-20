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

  it('focuses on "/" without typing it', async () => {
    const query = await setup('')

    await userEvent.keyboard('/')

    expect(document.activeElement).toBe(screen.getByRole('searchbox'))
    expect(query()).toBe('')
  })

  it('leaves "/" alone while the field is out of the document', async () => {
    const query = await setup('')
    const field = screen.getByRole('searchbox')
    const parent = field.parentElement!
    parent.removeChild(field)

    const slash = new KeyboardEvent('keydown', { key: '/', bubbles: true, cancelable: true })
    document.body.dispatchEvent(slash)

    expect(slash.defaultPrevented).toBe(false)
    parent.appendChild(field)
    expect(query()).toBe('')
  })
})
