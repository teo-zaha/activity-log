import { signal } from '@angular/core'
import { type RenderResult, fireEvent, render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import type { MaximizeButtonState } from '../../generated/activity'
import { WindowChromeService } from '../../services/window-chrome.service'
import { WindowControlsComponent } from './window-controls.component'

function fakeChrome(): {
  maximized: ReturnType<typeof signal<boolean>>
  maximizeButton: ReturnType<typeof signal<MaximizeButtonState>>
  attachMaximizeButton: ReturnType<typeof vi.fn>
  minimize: ReturnType<typeof vi.fn>
  toggleMaximize: ReturnType<typeof vi.fn>
  close: ReturnType<typeof vi.fn>
} {
  return {
    maximized: signal(false),
    maximizeButton: signal({ hovered: false, pressed: false }),
    attachMaximizeButton: vi.fn(() => Promise.resolve()),
    minimize: vi.fn(() => Promise.resolve()),
    toggleMaximize: vi.fn(() => Promise.resolve()),
    close: vi.fn(() => Promise.resolve()),
  }
}

async function renderControls(): Promise<
  RenderResult<WindowControlsComponent> & { chrome: ReturnType<typeof fakeChrome> }
> {
  const chrome = fakeChrome()
  return render(WindowControlsComponent, {
    providers: [{ provide: WindowChromeService, useValue: chrome }],
  }).then(result => ({ ...result, chrome }))
}

describe('WindowControlsComponent', () => {
  it('minimizes, maximizes or restores, and closes the window', async () => {
    const { fixture, chrome } = await renderControls()

    await userEvent.click(screen.getByRole('button', { name: 'Minimize' }))
    await userEvent.click(screen.getByRole('button', { name: 'Maximize' }))
    await userEvent.click(screen.getByRole('button', { name: 'Close' }))
    expect(chrome.minimize).toHaveBeenCalledOnce()
    expect(chrome.toggleMaximize).toHaveBeenCalledOnce()
    expect(chrome.close).toHaveBeenCalledOnce()

    chrome.maximized.set(true)
    await fixture.whenStable()
    expect(screen.getByRole('button', { name: 'Restore' })).toBeTruthy()
  })

  it('hands the maximize button to the snap layouts and shows the pointer the app reports', async () => {
    const { fixture, chrome } = await renderControls()
    const maximize = screen.getByRole('button', { name: 'Maximize' })
    expect(chrome.attachMaximizeButton).toHaveBeenCalledWith(maximize)

    chrome.maximizeButton.set({ hovered: true, pressed: true })
    await fixture.whenStable()
    expect(maximize.classList).toContain('is-hovered')
    expect(maximize.classList).toContain('is-pressed')
  })

  it('dims the buttons while the window is in the background', async () => {
    const { fixture } = await renderControls()
    const host = fixture.nativeElement as HTMLElement

    fireEvent.focus(window)
    await fixture.whenStable()
    expect(host.classList).not.toContain('is-inactive')

    fireEvent.blur(window)
    await fixture.whenStable()
    expect(host.classList).toContain('is-inactive')
  })
})
