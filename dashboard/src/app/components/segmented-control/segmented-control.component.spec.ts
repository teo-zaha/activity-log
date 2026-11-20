import { signal } from '@angular/core'
import { createEvent, fireEvent, render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { type SegmentedOption, SegmentedControlComponent } from './segmented-control.component'

type Level = 'all' | 'warn' | 'error'

const OPTIONS: readonly SegmentedOption<Level>[] = [
  { value: 'all', label: 'All', count: 4 },
  { value: 'warn', label: 'Warnings', count: 1, tone: 'warn' },
  { value: 'error', label: 'Errors', count: 2, tone: 'error' },
]

async function renderControl(): Promise<ReturnType<typeof signal<Level>>> {
  const value = signal<Level>('all')
  await render(
    `<app-segmented-control label="Minimum log level" [options]="options" [(value)]="value" />`,
    { imports: [SegmentedControlComponent], componentProperties: { options: OPTIONS, value } }
  )
  return value
}

const radio = (name: RegExp): HTMLElement => screen.getByRole('radio', { name })

describe('SegmentedControlComponent', () => {
  it('is a labelled radio group with only the checked option in the tab order', async () => {
    await renderControl()

    expect(screen.getByRole('radiogroup', { name: 'Minimum log level' })).toBeTruthy()
    expect(radio(/All/).getAttribute('aria-checked')).toBe('true')
    expect(radio(/All/).tabIndex).toBe(0)
    expect(radio(/Warnings/).tabIndex).toBe(-1)
    expect(radio(/Errors/).textContent).toContain('2')
  })

  it('selects on click', async () => {
    const value = await renderControl()

    await userEvent.click(radio(/Errors/))

    expect(value()).toBe('error')
    expect(radio(/Errors/).getAttribute('aria-checked')).toBe('true')
  })

  it('moves the choice and focus with the arrow keys, wrapping, and Home/End', async () => {
    const value = await renderControl()
    const group = screen.getByRole('radiogroup')

    fireEvent.keyDown(group, { key: 'ArrowRight' })
    expect(value()).toBe('warn')
    expect(document.activeElement).toBe(radio(/Warnings/))

    fireEvent.keyDown(group, { key: 'ArrowLeft' })
    fireEvent.keyDown(group, { key: 'ArrowLeft' })
    expect(value()).toBe('error')

    fireEvent.keyDown(group, { key: 'Home' })
    expect(value()).toBe('all')
    fireEvent.keyDown(group, { key: 'End' })
    expect(value()).toBe('error')
  })

  it('leaves a modified arrow (Alt+← is Back) to the app and the browser', async () => {
    const value = await renderControl()
    const group = screen.getByRole('radiogroup')

    for (const modifier of ['altKey', 'ctrlKey', 'metaKey'] as const) {
      const event = createEvent.keyDown(group, { key: 'ArrowLeft', [modifier]: true })
      fireEvent(group, event)
      expect(event.defaultPrevented, modifier).toBe(false)
    }

    expect(value()).toBe('all')
  })
})
