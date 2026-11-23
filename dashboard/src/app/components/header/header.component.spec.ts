import { signal } from '@angular/core'
import { provideRouter } from '@angular/router'
import { fireEvent, render, screen, within } from '@testing-library/angular'
import { describe, expect, it, vi } from 'vitest'
import type { ActivityLog } from '../../generated/activity'
import { ActivityExportService } from '../../services/activity-export.service'
import { WindowChromeService } from '../../services/window-chrome.service'
import { activityLog } from '../../testing/activity-fixtures'
import { HeaderComponent } from './header.component'

/** The window's frame as the header sees it; its window calls fail the spec if made. */
function chromeMoving(movesWindow: boolean): Partial<WindowChromeService> {
  const unexpected = vi.fn(() => Promise.reject(new Error('the header calls no window API')))
  return { movesWindow: signal(movesWindow), toggleMaximize: unexpected }
}

async function setup(
  state: {
    loading?: boolean
    meta?: ActivityLog['meta']
    chrome?: Partial<WindowChromeService>
  } = {}
): Promise<void> {
  await render(HeaderComponent, {
    providers: [
      provideRouter([]),
      {
        provide: ActivityExportService,
        useValue: {
          canExport: signal(state.meta !== undefined && !state.loading),
        },
      },
      { provide: WindowChromeService, useValue: state.chrome ?? chromeMoving(false) },
    ],
  })
}

describe('HeaderComponent export', () => {
  const exportButtons = (): HTMLElement[] =>
    ['Export Markdown', 'Export JSON'].map(name => screen.getByRole('button', { name }))

  it('is offered for a collected log', async () => {
    await setup({ meta: activityLog.meta })

    expect(exportButtons().map(button => button.hasAttribute('disabled'))).toEqual([false, false])
  })

  it('are the only actions: there is no Share', async () => {
    await setup({ meta: activityLog.meta })

    const actions = screen.getByRole('group', { name: 'Export' })
    expect(within(actions).getAllByRole('button')).toEqual(exportButtons())
    expect(screen.queryByRole('button', { name: /share/i })).toBeNull()
  })

  it('waits until a running collection is complete', async () => {
    await setup({ meta: activityLog.meta, loading: true })

    expect(exportButtons().map(button => button.hasAttribute('disabled'))).toEqual([true, true])
  })
})

describe('HeaderComponent title bar', () => {
  const DRAG = 'data-tauri-drag-region'
  /** Every element Tauri drags the window from, by class, and how. */
  const dragRegions = (): [string, string][] =>
    [...document.querySelectorAll(`[${DRAG}]`)].map(element => [
      element.className,
      element.getAttribute(DRAG) ?? '',
    ])

  it('moves the window from the strip, the row, its empty middle and the brand', async () => {
    await setup({ meta: activityLog.meta, chrome: chromeMoving(true) })

    expect(dragRegions()).toEqual([
      ['window-drag-strip', ''],
      ['header-inner', ''],
      ['brand', 'deep'],
      ['header-center', ''],
    ])
    for (const button of screen.getAllByRole('button')) {
      expect(button.closest(`[${DRAG}="deep"]`)).toBeNull()
    }
  })

  it('moves no window in fullscreen or in a browser', async () => {
    await setup({ meta: activityLog.meta, chrome: chromeMoving(false) })

    expect(dragRegions()).toEqual([])
  })

  it('leaves pressing and double-clicking to Tauri', async () => {
    const chrome = chromeMoving(true)
    await setup({ meta: activityLog.meta, chrome })
    const row = document.querySelector('.header-inner')!

    fireEvent.mouseDown(row)
    fireEvent.dblClick(row)
    fireEvent.mouseDown(document.querySelector('.window-drag-strip')!)

    expect(chrome.toggleMaximize).not.toHaveBeenCalled()
  })
})
