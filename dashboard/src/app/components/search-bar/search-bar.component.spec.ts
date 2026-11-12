import { signal, type WritableSignal } from '@angular/core'
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
    expect(await screen.findByDisplayValue('xyz')).toBe(input)
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

  it('focuses on "/" without typing it', async () => {
    const query = await setup('')

    await userEvent.keyboard('/')

    expect(document.activeElement).toBe(screen.getByRole('searchbox'))
    expect(query()).toBe('')
  })
})
