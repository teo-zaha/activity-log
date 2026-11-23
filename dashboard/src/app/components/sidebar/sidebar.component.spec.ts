import { signal } from '@angular/core'
import { TestbedHarnessEnvironment } from '@angular/cdk/testing/testbed'
import { provideLocationMocks } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { MatTooltipHarness } from '@angular/material/tooltip/testing'
import { Router } from '@angular/router'
import { render, screen, within } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { provideAppRouter } from '../../app.routes'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { BreakpointState } from '../../services/breakpoint-state.service'
import {
  REVEAL_DELAY_MS,
  SHORTCUT_HELP_DIALOG_ID,
  SHORTCUT_PLATFORM,
  ShortcutService,
} from '../../services/shortcut.service'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { SidebarComponent } from './sidebar.component'

describe('SidebarComponent', () => {
  it('links the views, keeping the range and the day, and marks the current one', async () => {
    await render(SidebarComponent, {
      providers: [
        provideAppRouter(),
        provideLocationMocks(),
        {
          provide: ActivityViewStateService,
          useValue: { selectedDay: signal('2026-11-10') },
        },
        { provide: SettingsDialogLauncher, useValue: { open: vi.fn(), pageOpen: signal(false) } },
      ],
    })
    const router = TestBed.inject(Router)
    await router.navigateByUrl('/week/2026-11-10?q=review&from=2026-11-01&to=2026-11-14')

    const views = screen.getByRole('navigation', { name: 'Views' })
    const link = (name: string): HTMLElement => within(views).getByRole('link', { name })
    await vi.waitFor(() => expect(link('Week').getAttribute('aria-current')).toBe('page'))
    await vi.waitFor(() => {
      expect(link('Timeline').getAttribute('href')).toBe('/timeline?from=2026-11-01&to=2026-11-14')
      expect(link('Day').getAttribute('href')).toBe('/day/2026-11-10?from=2026-11-01&to=2026-11-14')
      expect(link('Timeline').getAttribute('aria-current')).toBeNull()
    })

    await userEvent.click(link('Day'))
    await vi.waitFor(() => expect(router.url).toBe('/day/2026-11-10?from=2026-11-01&to=2026-11-14'))
  })

  it('lays itself out as the shared breakpoint state says, as the source list does', async () => {
    const { container } = await render(SidebarComponent, {
      providers: [
        provideAppRouter(),
        provideLocationMocks(),
        { provide: ActivityViewStateService, useValue: { selectedDay: signal('2026-11-10') } },
        { provide: SettingsDialogLauncher, useValue: { open: vi.fn(), pageOpen: signal(false) } },
        { provide: BreakpointState, useValue: { phone: signal(false), rail: signal(true) } },
      ],
    })

    expect(container.querySelector('.sidebar')?.classList.contains('is-rail')).toBe(true)
  })

  it('keeps Settings out of what scrolls in a short window, in the rail too', async () => {
    const { container } = await render(SidebarComponent, {
      providers: [
        provideAppRouter(),
        provideLocationMocks(),
        { provide: ActivityViewStateService, useValue: { selectedDay: signal('2026-11-10') } },
        { provide: SettingsDialogLauncher, useValue: { open: vi.fn(), pageOpen: signal(false) } },
        { provide: BreakpointState, useValue: { phone: signal(false), rail: signal(true) } },
      ],
    })

    const scroller = container.querySelector('.sidebar-scroll')
    expect(scroller?.contains(screen.getByRole('navigation', { name: 'Views' }))).toBe(true)
    const settings = screen.getByRole('button', { name: 'Settings' })
    expect(scroller?.contains(settings)).toBe(false)
    expect(container.querySelector('.sidebar')?.contains(settings)).toBe(true)
  })

  describe('Keyboard shortcuts', () => {
    /** Shift pressed on its own and held: the tags show after `REVEAL_DELAY_MS`. */
    function holdShift(): void {
      document.body.dispatchEvent(
        new KeyboardEvent('keydown', { key: 'Shift', shiftKey: true, bubbles: true })
      )
    }

    const HINT = 'Hold Shift to see shortcuts, let go to use them'

    /**
     * The sidebar with the commands its entries run, as the shell's fallbacks give them, and the
     * help entry's tooltip, through Material's own harness.
     */
    async function renderSidebar(
      layout: { phone: boolean; rail: boolean },
      os: 'mac' | 'other' = 'mac'
    ): Promise<{ tooltip: () => Promise<MatTooltipHarness> }> {
      const { fixture } = await render(SidebarComponent, {
        providers: [
          provideAppRouter(),
          provideLocationMocks(),
          { provide: SHORTCUT_PLATFORM, useValue: { os, build: 'browser' } },
          { provide: ActivityViewStateService, useValue: { selectedDay: signal('2026-11-10') } },
          { provide: SettingsDialogLauncher, useValue: { open: vi.fn(), pageOpen: signal(false) } },
          {
            provide: BreakpointState,
            useValue: { phone: signal(layout.phone), rail: signal(layout.rail) },
          },
        ],
      })
      const shortcuts = TestBed.inject(ShortcutService)
      for (const id of ['view.list', 'view.day', 'view.workweek', 'view.week'] as const) {
        shortcuts.fallback(id, vi.fn())
      }
      shortcuts.fallback('app.settings', vi.fn())
      shortcuts.fallback('app.help', vi.fn())
      const loader = TestbedHarnessEnvironment.loader(fixture)
      return {
        tooltip: () => loader.getHarness(MatTooltipHarness.with({ selector: '.help-button' })),
      }
    }

    it('opens the help beside Settings, out of what scrolls, naming and showing its key', async () => {
      await renderSidebar({ phone: false, rail: false })
      const entry = screen.getByRole('button', {
        name: 'Keyboard shortcuts',
        description: HINT,
      })
      expect(entry.getAttribute('aria-keyshortcuts')).toBe('?')
      expect(document.querySelector('.sidebar-scroll')?.contains(entry)).toBe(false)
      expect(entry.parentElement?.contains(screen.getByRole('button', { name: 'Settings' }))).toBe(
        true
      )

      await userEvent.click(entry)
      const dialogs = TestBed.inject(MatDialog)
      expect(dialogs.getDialogById(SHORTCUT_HELP_DIALOG_ID)).toBeDefined()
      // A second click leaves it open: the entry opens the help, it does not toggle it.
      await userEvent.click(entry)
      expect(dialogs.openDialogs).toHaveLength(1)

      dialogs.getDialogById(SHORTCUT_HELP_DIALOG_ID)?.close()
      // As the help closes, focus goes back to the entry that opened it.
      await vi.waitFor(() => expect(document.activeElement).toBe(entry))
    })

    it('shows Shift at rest, as ⇧ on macOS, and its own key while Shift is held', async () => {
      await renderSidebar({ phone: false, rail: false })
      const tag = screen
        .getByRole('button', { name: 'Keyboard shortcuts' })
        .querySelector('app-key-tag')
      expect(tag?.textContent).toBe('⇧')
      expect(tag?.classList.contains('is-shown')).toBe(true)

      holdShift()
      await vi.waitFor(() => expect(tag?.textContent).toBe('?'))
      expect(tag?.classList.contains('is-shown')).toBe(true)
    })

    it('writes Shift out on other systems', async () => {
      await renderSidebar({ phone: false, rail: false }, 'other')
      const entry = screen.getByRole('button', { name: 'Keyboard shortcuts' })
      expect(entry.querySelector('app-key-tag')?.textContent).toBe('Shift')
    })

    it('is an icon in the rail, with its Shift tag, its hint saying no name twice', async () => {
      const { tooltip } = await renderSidebar({ phone: false, rail: true })
      const entry = screen.getByRole('button', { name: 'Keyboard shortcuts', description: HINT })
      expect(entry.getAttribute('aria-keyshortcuts')).toBe('?')
      expect(entry.querySelector('app-key-tag')?.classList.contains('is-shown')).toBe(true)

      const hint = await tooltip()
      expect(await hint.isDisabled()).toBe(false)
      await hint.show()
      expect(await hint.getTooltipText()).toBe(HINT)
    })

    it('says beside its label on wider screens that Shift shows the keys', async () => {
      const { tooltip } = await renderSidebar({ phone: false, rail: false })
      const hint = await tooltip()
      await hint.show()
      expect(await hint.getTooltipText()).toBe(HINT)
    })

    it('promises nothing where holding Shift shows no key: single keys off in the browser', async () => {
      localStorage.setItem('character-shortcuts', 'off')
      try {
        const { tooltip } = await renderSidebar({ phone: false, rail: false })
        const entry = screen.getByRole('button', { name: 'Keyboard shortcuts' })
        expect(entry.querySelector('app-key-tag')?.classList.contains('is-shown')).toBe(false)
        expect(await (await tooltip()).isDisabled()).toBe(true)
        holdShift()
        await new Promise(resolve => setTimeout(resolve, REVEAL_DELAY_MS + 50))
        expect(entry.querySelector('app-key-tag')?.classList.contains('is-shown')).toBe(false)
      } finally {
        localStorage.clear()
      }
    })

    it('names the rail’s entry by its tooltip where holding Shift shows no key', async () => {
      localStorage.setItem('character-shortcuts', 'off')
      try {
        const { tooltip } = await renderSidebar({ phone: false, rail: true })
        const hint = await tooltip()
        await hint.show()
        expect(await hint.getTooltipText()).toBe('Keyboard shortcuts')
      } finally {
        localStorage.clear()
      }
    })

    it('tags the views and Settings with their keys, shown only while Shift is held', async () => {
      await renderSidebar({ phone: false, rail: true })
      const views = screen.getByRole('navigation', { name: 'Views' })
      const tagOf = (element: HTMLElement): Element | null => element.querySelector('app-key-tag')
      const links = within(views).getAllByRole('link')
      const settings = screen.getByRole('button', { name: 'Settings' })
      expect(links.map(link => tagOf(link)?.textContent)).toEqual(['1', '2', '3', '4'])
      expect(tagOf(settings)?.textContent).toBe(',')
      const shown = (): boolean[] =>
        [...links, settings].map(element => tagOf(element)?.classList.contains('is-shown') ?? false)
      expect(shown().every(is => !is)).toBe(true)

      holdShift()
      await vi.waitFor(() => expect(shown().every(is => is)).toBe(true))
    })

    it('is not in the phone’s bottom bar, where Settings › General has it', async () => {
      await renderSidebar({ phone: true, rail: false })
      expect(screen.queryByRole('button', { name: 'Keyboard shortcuts' })).toBeNull()
    })
  })
})
