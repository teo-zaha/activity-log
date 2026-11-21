import { Location } from '@angular/common'
import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { fireEvent, screen, within } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { createEntryAdapter } from '../../providers/provider-adapter'
import { ActivityDataService } from '../../services/activity-data.service'
import type { ShortcutPlatform } from '../../services/shortcut.service'
import { activityLog, MockBackendTransport } from '../../testing/activity-fixtures'
import { type OpenApp, openApp } from '../../testing/app'
import { mockPhone } from '../../testing/media'
import { APP_RENDER_TEST } from '../../testing/timeouts'

const MAC_DESKTOP: ShortcutPlatform = { os: 'mac', build: 'desktop' }
const WINDOWS_DESKTOP: ShortcutPlatform = { os: 'other', build: 'desktop' }
const BROWSER: ShortcutPlatform = { os: 'other', build: 'browser' }

/** For what loads its code on demand (the settings, the views), slow on a cold build cache. */
const SLOW = { timeout: 10_000 }
const SLOW_TEST = 30_000

const [git, jira] = activityLog.entries.map(entry => createEntryAdapter(entry))
const key = (entry: { trackKey: string } | undefined): string =>
  encodeURIComponent(entry?.trackKey ?? '')

/** A key press where focus is, as the browser sends it. */
function press(key: string, init: KeyboardEventInit = {}): KeyboardEvent {
  const target = document.activeElement ?? document.body
  const event = new KeyboardEvent('keydown', { key, bubbles: true, cancelable: true, ...init })
  fireEvent(target, event)
  return event
}

const filter = (): HTMLElement => screen.getByRole('searchbox', { name: 'Filter activity' })
const collectButton = (): HTMLElement => screen.getByRole('button', { name: 'Collect activity' })

/** The app once its first log is in, so Collect is enabled. */
async function ready(url: string, platform: ShortcutPlatform): Promise<OpenApp> {
  const app = await openApp(url, { platform })
  const data = TestBed.inject(ActivityDataService)
  await vi.waitFor(() => expect(data.meta()).not.toBeNull())
  await app.settle()
  return app
}

/** The rows of the open help, each `label: keys`, keys as they read (`⌘R or R`). */
async function helpRows(): Promise<string[]> {
  const help = await screen.findByRole('dialog', { name: 'Keyboard shortcuts' })
  return within(help)
    .getAllByRole('term')
    .map(term => `${term.textContent}: ${term.nextElementSibling?.textContent?.trim()}`)
}

describe('Shortcuts in the app', APP_RENDER_TEST, () => {
  let phone: ReturnType<typeof mockPhone> | null = null
  afterEach(() => {
    phone?.restore()
    phone = null
    localStorage.clear()
  })

  describe('`/` and the Timeline’s filter', () => {
    it('focuses the filter without typing the key, and names it on the field', async () => {
      await openApp('/timeline', { platform: BROWSER })
      const field = await screen.findByRole('searchbox', { name: 'Filter activity' }, SLOW)

      expect(press('/').defaultPrevented).toBe(true)
      expect(document.activeElement).toBe(field)
      expect(field).toHaveProperty('value', '')
      expect(field.getAttribute('aria-keyshortcuts')).toBe('/')
      expect(field.parentElement?.querySelector('app-key-tag')?.textContent).toBe('/')
    })

    it('types `/` and `?` in the filter as text', async () => {
      await openApp('/timeline', { platform: BROWSER })
      const field = await screen.findByRole('searchbox', { name: 'Filter activity' }, SLOW)

      await userEvent.type(field, 'a?b/c')

      expect(field).toHaveProperty('value', 'a?b/c')
      expect(TestBed.inject(MatDialog).openDialogs).toHaveLength(0)
    })

    it('switches from a calendar view to the Timeline and focuses its filter, as ⌘F does', async () => {
      const { router, settle } = await openApp('/day/2026-05-18?from=2026-05-17&to=2026-05-18', {
        platform: BROWSER,
      })

      expect(press('/', { code: 'Slash' }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(router.url).toBe('/timeline?from=2026-05-17&to=2026-05-18'))
      await settle()

      await vi.waitFor(() => expect(document.activeElement).toBe(filter()), SLOW)
      expect(filter()).toHaveProperty('value', '')
    })

    it('does nothing under the phone’s Settings tab', async () => {
      phone = mockPhone(true)
      await openApp('/timeline?settings=home', { platform: BROWSER })
      await vi.waitFor(() => expect(screen.getByRole('region', { name: 'Settings' })).toBeTruthy())

      expect(press('/').defaultPrevented).toBe(false)
      // The view is out of sight under the tab, its filter too.
      expect(document.activeElement).not.toBe(document.querySelector('input[type="search"]'))
    })

    it(
      'does not reach the filter under the settings dialog',
      async () => {
        await openApp('/timeline?settings=general', { platform: BROWSER })
        await vi.waitFor(
          () => expect(document.querySelector('mat-dialog-container')).not.toBeNull(),
          SLOW
        )

        press('/')

        expect(document.activeElement).not.toBe(filter())
      },
      SLOW_TEST
    )
  })

  describe('⌘F / Ctrl+F', () => {
    it('switches from a calendar view to the Timeline, keeping the range, and focuses its filter', async () => {
      const { router, settle } = await openApp('/day/2026-05-18?from=2026-05-17&to=2026-05-18', {
        platform: MAC_DESKTOP,
      })

      expect(press('f', { metaKey: true }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(router.url).toBe('/timeline?from=2026-05-17&to=2026-05-18'))
      await settle()

      await vi.waitFor(() => expect(document.activeElement).toBe(filter()), SLOW)
    })

    it('focuses the filter on the Timeline, also from another field', async () => {
      await openApp('/timeline', { platform: WINDOWS_DESKTOP })
      await screen.findByRole('searchbox', { name: 'Filter activity' }, SLOW)
      screen.getByLabelText('From date').focus()

      expect(press('f', { ctrlKey: true }).defaultPrevented).toBe(true)

      expect(document.activeElement).toBe(filter())
    })

    it.each(['/timeline', '/day/2026-05-18'])(
      'leaves the key alone when there is no filter to focus: no sources, on %s',
      async url => {
        const backend = new MockBackendTransport()
        backend.providersResponse = { currentOs: 'macos', providers: [] }
        const { router, settle } = await openApp(url, { backend, platform: WINDOWS_DESKTOP })
        await vi.waitFor(() =>
          expect(TestBed.inject(ActivityDataService).hasNoSources()).toBe(true)
        )
        await settle()
        const from = screen.getByLabelText('From date')
        from.focus()

        expect(press('f', { ctrlKey: true }).defaultPrevented).toBe(false)
        expect(router.url).toBe(url)
        expect(document.activeElement).toBe(from)
      }
    )

    it('stays the browser’s own find', async () => {
      const { router } = await openApp('/day/2026-05-18', { platform: BROWSER })

      expect(press('f', { ctrlKey: true }).defaultPrevented).toBe(false)
      expect(router.url).toBe('/day/2026-05-18')
    })

    it('does not leave open details for the filter under them', async () => {
      const { router } = await openApp(`/day/2026-05-18?event=${key(git)}`, {
        platform: MAC_DESKTOP,
      })
      await screen.findByRole('heading', { name: git?.title }, SLOW)

      expect(press('f', { metaKey: true }).defaultPrevented).toBe(false)
      expect(router.url).toBe(`/day/2026-05-18?event=${key(git)}`)
    })
  })

  describe('The views: ⌘1 to ⌘4, and 1 to 4', () => {
    it('switches the view, keeping the range', async () => {
      const { router } = await openApp('/day/2026-05-18', { platform: MAC_DESKTOP })

      expect(press('4', { metaKey: true }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(router.url).toBe('/week/2026-05-18'))
    })

    it('switches the view with the digit alone in the browser, and names it on the links', async () => {
      const { router } = await openApp('/timeline?from=2026-05-17&to=2026-05-18', {
        platform: BROWSER,
      })
      const views = screen.getByRole('navigation', { name: 'Views' })
      expect(
        within(views).getByRole('link', { name: 'Day' }).getAttribute('aria-keyshortcuts')
      ).toBe('2')

      expect(press('2', { code: 'Digit2' }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(router.url).toMatch(/^\/day\/.*from=2026-05-17&to=2026-05-18$/))
      expect(press('1', { code: 'Digit1' }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(router.url).toBe('/timeline?from=2026-05-17&to=2026-05-18'))
    })

    it('does not switch the view behind open details, as ⌘F does not', async () => {
      const { router } = await openApp(`/day/2026-05-18?event=${key(git)}`, {
        platform: MAC_DESKTOP,
      })
      await screen.findByRole('heading', { name: git?.title }, SLOW)

      expect(press('4', { metaKey: true }).defaultPrevented).toBe(false)
      expect(router.url).toBe(`/day/2026-05-18?event=${key(git)}`)
    })
  })

  describe('Collect: ⌘R, Ctrl+R and F5', () => {
    it('collects the range on screen with ⌘R on macOS, also while typing, and shows ⌘R', async () => {
      await ready('/timeline', MAC_DESKTOP)
      const collect = vi.spyOn(TestBed.inject(ActivityDataService), 'collect')

      expect(collectButton().getAttribute('aria-keyshortcuts')).toBe('Meta+R R')
      expect(collectButton().querySelector('app-key-tag')?.textContent).toBe('⌘R')

      filter().focus()
      expect(press('r', { metaKey: true }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(collect).toHaveBeenCalledWith('2026-05-17', '2026-05-18'))
      // Ctrl+R is not the Mac's chord.
      collect.mockClear()
      expect(press('r', { ctrlKey: true }).defaultPrevented).toBe(false)
      expect(collect).not.toHaveBeenCalled()
    })

    it('collects with Ctrl+R and F5 off macOS', async () => {
      await ready('/day/2026-05-18', WINDOWS_DESKTOP)
      const data = TestBed.inject(ActivityDataService)
      const collect = vi.spyOn(data, 'collect')

      expect(collectButton().getAttribute('aria-keyshortcuts')).toBe('Control+R F5 R')
      expect(collectButton().querySelector('app-key-tag')?.textContent).toBe('Ctrl+R')
      expect(press('F5').defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(collect).toHaveBeenCalledOnce())
      await vi.waitFor(() => expect(data.isLoading()).toBe(false))
      expect(press('r', { ctrlKey: true }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(collect).toHaveBeenCalledTimes(2))
    })

    it('collects where the Collect button is not shown: the phone’s Settings tab', async () => {
      phone = mockPhone(true)
      await ready('/timeline?settings=home', MAC_DESKTOP)
      await vi.waitFor(() => expect(screen.getByRole('region', { name: 'Settings' })).toBeTruthy())
      expect(screen.queryByRole('button', { name: 'Collect activity' })).toBeNull()
      const collect = vi.spyOn(TestBed.inject(ActivityDataService), 'collect')

      expect(press('r', { metaKey: true }).defaultPrevented).toBe(true)

      await vi.waitFor(() => expect(collect).toHaveBeenCalledOnce())
    })

    it('collects the dates typed in a field that still has focus, as a click does', async () => {
      const { backend } = await ready('/timeline?from=2026-05-17&to=2026-05-18', WINDOWS_DESKTOP)
      const from = screen.getByLabelText('From date')
      const start = (day: number): string => new Date(2026, 4, day).toISOString()
      const lastCollect = (): unknown => backend.requests.at(-1)

      await userEvent.clear(from)
      await userEvent.type(from, '15/05/2026')
      expect(press('r', { ctrlKey: true }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(lastCollect()).toMatchObject({ start: start(15) }))
      await vi.waitFor(() => expect(TestBed.inject(ActivityDataService).isLoading()).toBe(false))

      // A date that is not one (31 February) leaves the range as it was.
      await userEvent.clear(from)
      await userEvent.type(from, '31/02/2026')
      press('r', { ctrlKey: true })
      await vi.waitFor(() => expect(lastCollect()).toMatchObject({ start: start(15) }))
      await vi.waitFor(() => expect(TestBed.inject(ActivityDataService).isLoading()).toBe(false))

      await userEvent.clear(from)
      await userEvent.type(from, '16/05/2026')
      collectButton().click()
      await vi.waitFor(() => expect(lastCollect()).toMatchObject({ start: start(16) }))
    })

    it('leaves every key to an open date picker, its own Escape included', async () => {
      await ready('/timeline', MAC_DESKTOP)
      await screen.findByRole('searchbox', { name: 'Filter activity' }, SLOW)
      const collect = vi.spyOn(TestBed.inject(ActivityDataService), 'collect')
      await userEvent.click(screen.getAllByRole('button', { name: 'Open calendar' })[0])
      await screen.findByRole('dialog', {}, SLOW)

      press('/')
      press('?')
      press('r', { metaKey: true })

      expect(document.activeElement).not.toBe(filter())
      expect(screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })).toBeNull()
      expect(collect).not.toHaveBeenCalled()
      // The overlay reads Escape by its key code, as browsers still send it.
      const escape = press('Escape', { keyCode: 27 })
      expect(escape.defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(screen.queryByRole('dialog')).toBeNull(), SLOW)
      expect(press('/').defaultPrevented).toBe(true)
      expect(document.activeElement).toBe(filter())
    })

    it('leaves the browser its reload, and collects with R there', async () => {
      await ready('/timeline', BROWSER)
      const collect = vi.spyOn(TestBed.inject(ActivityDataService), 'collect')

      expect(press('r', { ctrlKey: true, code: 'KeyR' }).defaultPrevented).toBe(false)
      expect(press('F5').defaultPrevented).toBe(false)
      expect(collect).not.toHaveBeenCalled()
      expect(collectButton().getAttribute('aria-keyshortcuts')).toBe('R')
      expect(collectButton().querySelector('app-key-tag')?.textContent).toBe('R')

      expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(collect).toHaveBeenCalledOnce())
      // Not from a field, where R types.
      await vi.waitFor(() => expect(TestBed.inject(ActivityDataService).isLoading()).toBe(false))
      filter().focus()
      expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(false)
      expect(collect).toHaveBeenCalledOnce()
    })

    it('shows ⌘R on the desktop app’s button, and takes R there too', async () => {
      await ready('/timeline', MAC_DESKTOP)
      const collect = vi.spyOn(TestBed.inject(ActivityDataService), 'collect')

      expect(collectButton().getAttribute('aria-keyshortcuts')).toBe('Meta+R R')
      expect(collectButton().querySelector('app-key-tag')?.textContent).toBe('⌘R')
      expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(collect).toHaveBeenCalledOnce())
    })
  })

  describe('Settings: ⌘, and ,', () => {
    it(
      'opens the settings with `,` in the browser, named on the sidebar’s button',
      async () => {
        const { router } = await openApp('/timeline', { platform: BROWSER })
        expect(
          screen.getByRole('button', { name: 'Settings' }).getAttribute('aria-keyshortcuts')
        ).toBe(',')

        expect(press(',', { code: 'Comma' }).defaultPrevented).toBe(true)
        await vi.waitFor(() => expect(router.url).toBe('/timeline?settings=general'))
        await screen.findByRole('heading', { name: 'General' }, SLOW)
        await userEvent.click(screen.getByRole('button', { name: 'Close settings' }))
        await vi.waitFor(() => expect(router.url).toBe('/timeline'))
        await vi.waitFor(() => expect(TestBed.inject(MatDialog).openDialogs).toHaveLength(0))
      },
      SLOW_TEST
    )
  })

  describe('The calendar: P, N and T', () => {
    it('moves the Day view by a day, and goes to today', async () => {
      const { router, settle } = await openApp('/day/2026-05-18', { platform: BROWSER })
      await settle()
      const today = screen.getByRole('button', { name: /Today/ })
      expect(today.getAttribute('aria-keyshortcuts')).toBe('T')
      expect(today.querySelector('app-key-tag')?.textContent).toBe('T')
      expect(
        screen.getByRole('button', { name: 'Next day' }).getAttribute('aria-keyshortcuts')
      ).toBe('N')

      expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(router.url).toBe('/day/2026-05-19'))
      await settle()
      press('p', { code: 'KeyP' })
      await vi.waitFor(() => expect(router.url).toBe('/day/2026-05-18'))
      await settle()
      press('p', { code: 'KeyP' })
      await vi.waitFor(() => expect(router.url).toBe('/day/2026-05-17'))
      await settle()
      press('t', { code: 'KeyT' })
      await vi.waitFor(() => expect(router.url).not.toBe('/day/2026-05-17'))
      expect(router.url).toMatch(/^\/day\/\d{4}-\d{2}-\d{2}$/)
      await settle()
    })

    it('moves the Week view by a week', async () => {
      const { router, settle } = await openApp('/week/2026-05-18', { platform: BROWSER })
      await settle()

      expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(router.url).toBe('/week/2026-05-25'))
    })

    it('does nothing on the Timeline', async () => {
      const { router } = await openApp('/timeline', { platform: BROWSER })
      await screen.findByRole('searchbox', { name: 'Filter activity' }, SLOW)

      expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(false)
      expect(press('t', { code: 'KeyT' }).defaultPrevented).toBe(false)
      expect(router.url).toBe('/timeline')
    })
  })

  describe('The details: P and N', () => {
    const group = `/week/2026-05-18?events=${key(git)}&events=${key(jira)}`

    it('steps through a group, not the week behind it, and declines at its ends', async () => {
      const { router } = await openApp(`${group}&event=${key(git)}`, { platform: BROWSER })
      await screen.findByRole('heading', { name: git?.title }, SLOW)
      expect(
        screen.getByRole('button', { name: 'Next event' }).getAttribute('aria-keyshortcuts')
      ).toBe('N')

      expect(press('p', { code: 'KeyP' }).defaultPrevented).toBe(false)
      expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(true)
      await screen.findByRole('heading', { name: jira?.title }, SLOW)
      expect(router.parseUrl(router.url)).toEqual(router.parseUrl(`${group}&event=${key(jira)}`))
      expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(false)
      // Today is the calendar's: it does nothing under the details.
      expect(press('t', { code: 'KeyT' }).defaultPrevented).toBe(false)
      expect(router.url.startsWith('/week/2026-05-18')).toBe(true)
    })

    it('does nothing in the group’s list', async () => {
      const { router } = await openApp(group, { platform: BROWSER })
      await screen.findByRole('dialog', {}, SLOW)

      expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(false)
      expect(router.parseUrl(router.url)).toEqual(router.parseUrl(group))
    })

    it('steps the details over the phone’s Settings tab, opened under them', async () => {
      phone = mockPhone(true)
      const { router } = await openApp(`${group}&event=${key(git)}`, { platform: BROWSER })
      await screen.findByRole('heading', { name: git?.title }, SLOW)
      // `,` opens the Settings tab, which goes under the details.
      expect(press(',', { code: 'Comma' }).defaultPrevented).toBe(true)
      await vi.waitFor(() =>
        expect(router.parseUrl(router.url).queryParams['settings']).toBeDefined()
      )
      await screen.findByRole('region', { name: 'Settings' }, SLOW)

      expect(press('n', { code: 'KeyN' }).defaultPrevented).toBe(true)
      await vi.waitFor(() =>
        expect(router.parseUrl(router.url).queryParams['event']).toBe(jira?.trackKey)
      )
    })
  })

  describe('Single-key shortcuts off', () => {
    it(
      'turns them off from Settings › General, keeping ⌘R and Escape, and the help says so',
      async () => {
        const { router } = await ready('/timeline?settings=general', MAC_DESKTOP)
        const toggle = await screen.findByRole('switch', { name: 'Single-key shortcuts' }, SLOW)
        expect(toggle.getAttribute('aria-checked')).toBe('true')
        const helpButton = screen.getByRole('button', { name: /Keyboard shortcuts/ })
        expect(helpButton.querySelector('app-key-tag')?.textContent).toBe('?')

        await userEvent.click(toggle)
        expect(toggle.getAttribute('aria-checked')).toBe('false')
        expect(localStorage.getItem('character-shortcuts')).toBe('off')
        expect(helpButton.querySelector('app-key-tag')?.hasAttribute('hidden')).toBe(true)

        // The help, from its row: over the settings, without the keys that are off.
        await userEvent.click(helpButton)
        expect(await helpRows()).toEqual([
          'Collect: ⌘R',
          'Filter Activity…: ⌘F',
          'Timeline: ⌘1',
          'Day: ⌘2',
          'Work week: ⌘3',
          'Week: ⌘4',
          'Settings…: ⌘,',
          'Back: ⌘[',
          'Close details or settings: Esc',
        ])
        const help = screen.getByRole('dialog', { name: 'Keyboard shortcuts' })
        expect(within(help).getByText(/Single-key shortcuts are off/)).toBeTruthy()
        await userEvent.click(within(help).getByRole('button', { name: 'Close' }))
        await vi.waitFor(() =>
          expect(screen.queryByRole('dialog', { name: 'Keyboard shortcuts' })).toBeNull()
        )

        await userEvent.click(screen.getByRole('button', { name: 'Close settings' }))
        await vi.waitFor(() => expect(router.url).toBe('/timeline'))
        expect(press('?', { shiftKey: true, code: 'Slash' }).defaultPrevented).toBe(false)
        expect(press('2', { code: 'Digit2' }).defaultPrevented).toBe(false)
        expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(false)
        // Once the settings dialog has left, which hides the page from the tree meanwhile.
        await vi.waitFor(() =>
          expect(collectButton().getAttribute('aria-keyshortcuts')).toBe('Meta+R')
        )
        const collect = vi.spyOn(TestBed.inject(ActivityDataService), 'collect')
        expect(press('r', { metaKey: true, code: 'KeyR' }).defaultPrevented).toBe(true)
        await vi.waitFor(() => expect(collect).toHaveBeenCalledOnce())
        expect(router.url).toBe('/timeline')
      },
      SLOW_TEST
    )

    it('leaves the browser no key but Escape, and no tag on its controls', async () => {
      localStorage.setItem('character-shortcuts', 'off')
      await ready('/timeline', BROWSER)

      expect(press('r', { code: 'KeyR' }).defaultPrevented).toBe(false)
      expect(press('/', { code: 'Slash' }).defaultPrevented).toBe(false)
      expect(collectButton().hasAttribute('aria-keyshortcuts')).toBe(false)
      expect(collectButton().querySelector('app-key-tag')?.hasAttribute('hidden')).toBe(true)
      expect(filter().hasAttribute('aria-keyshortcuts')).toBe(false)
    })
  })

  describe('Back: ⌘[ and Alt+←', () => {
    it('goes from an event of a group to its list, then closes the details', async () => {
      const group = `/day/2026-05-18?events=${key(git)}&events=${key(jira)}`
      const { router } = await openApp(`${group}&event=${key(jira)}`, { platform: MAC_DESKTOP })
      await screen.findByRole('heading', { name: jira?.title }, SLOW)
      expect(
        screen.getByRole('button', { name: /overlapping events/ }).getAttribute('aria-keyshortcuts')
      ).toBe('Meta+[')

      expect(press('[', { metaKey: true }).defaultPrevented).toBe(true)
      await vi.waitFor(() => expect(router.parseUrl(router.url)).toEqual(router.parseUrl(group)))
      press('[', { metaKey: true })
      await vi.waitFor(() => expect(router.url).toBe('/day/2026-05-18'))
    })

    it(
      'leaves a settings section for the list, then the settings, on the phone’s tab',
      async () => {
        phone = mockPhone(true)
        const { router } = await openApp('/timeline?settings=logs', { platform: WINDOWS_DESKTOP })
        await screen.findByRole('heading', { name: 'Logs' }, SLOW)
        expect(
          screen.getByRole('button', { name: 'Back to settings' }).getAttribute('aria-keyshortcuts')
        ).toBe('Alt+ArrowLeft')

        press('ArrowLeft', { altKey: true })
        await vi.waitFor(() => expect(router.url).toBe('/timeline?settings=home'))
        press('ArrowLeft', { altKey: true })
        await vi.waitFor(() => expect(router.url).toBe('/timeline'))
      },
      SLOW_TEST
    )

    it('goes back in history with no layer open, and not while typing', async () => {
      const { router, settle } = await openApp('/timeline', { platform: WINDOWS_DESKTOP })
      await router.navigateByUrl('/day/2026-05-18')
      await settle()
      const back = vi.spyOn(TestBed.inject(Location), 'back')

      screen.getByLabelText('From date').focus()
      expect(press('ArrowLeft', { altKey: true }).defaultPrevented).toBe(false)
      if (document.activeElement instanceof HTMLElement) document.activeElement.blur()
      expect(press('ArrowLeft', { altKey: true }).defaultPrevented).toBe(true)

      expect(back).toHaveBeenCalledOnce()
    })

    it('is the browser’s own there', async () => {
      await openApp('/timeline', { platform: BROWSER })
      const back = vi.spyOn(TestBed.inject(Location), 'back')

      expect(press('ArrowLeft', { altKey: true }).defaultPrevented).toBe(false)
      expect(back).not.toHaveBeenCalled()
    })
  })

  describe('Close controls name their keys', () => {
    it(
      'the settings dialog’s close button says Escape',
      async () => {
        await openApp('/timeline?settings=logs', { platform: BROWSER })
        const close = await screen.findByRole('button', { name: 'Close settings' }, SLOW)
        expect(close.getAttribute('aria-keyshortcuts')).toBe('Escape')
      },
      SLOW_TEST
    )

    it('a single event’s back arrow on a phone says Escape and Back', async () => {
      phone = mockPhone(true)
      await openApp(`/day/2026-05-18?event=${key(git)}`, { platform: MAC_DESKTOP })
      const details = await screen.findByRole('dialog', { name: git?.title }, SLOW)
      const back = within(details).getByRole('button', { name: 'Close event details' })
      expect(back.getAttribute('aria-keyshortcuts')).toBe('Escape Meta+[')
    })

    it('says only Escape in the browser, which has no Back of the app’s', async () => {
      phone = mockPhone(true)
      await openApp(`/day/2026-05-18?event=${key(git)}`, { platform: BROWSER })
      const details = await screen.findByRole('dialog', { name: git?.title }, SLOW)
      const back = within(details).getByRole('button', { name: 'Close event details' })
      expect(back.getAttribute('aria-keyshortcuts')).toBe('Escape')
    })
  })

  describe('Escape', () => {
    it('closes the details only', async () => {
      const { router } = await openApp(`/day/2026-05-18?event=${key(git)}`, { platform: BROWSER })
      const details = await screen.findByRole('dialog', { name: git?.title }, SLOW)
      const close = within(details).getByRole('button', { name: 'Close event details' })
      expect(close.getAttribute('aria-keyshortcuts')).toBe('Escape')

      expect(press('Escape').defaultPrevented).toBe(true)

      await vi.waitFor(() => expect(router.url).toBe('/day/2026-05-18'))
      expect(press('Escape').defaultPrevented).toBe(false)
    })
  })

  describe('`?`, the help', () => {
    it('lists the shortcuts the browser has by where they work, and closes with `?` again', async () => {
      await openApp('/timeline', { platform: BROWSER })

      expect(press('?', { shiftKey: true }).defaultPrevented).toBe(true)
      const help = await screen.findByRole('dialog', { name: 'Keyboard shortcuts' })
      expect(
        within(help)
          .getAllByRole('heading', { level: 3 })
          .map(heading => heading.textContent)
      ).toEqual(['Everywhere', 'Timeline', 'Calendar', 'Event details'])
      expect(await helpRows()).toEqual([
        'Collect: R',
        'Filter Activity…: /',
        'Timeline: 1',
        'Day: 2',
        'Work week: 3',
        'Week: 4',
        'Settings…: ,',
        'Close details or settings: Esc',
        'Keyboard Shortcuts: ?',
        'Filter activity: /',
        'Previous day or week: P',
        'Next day or week: N',
        'Today: T',
        'Previous event: P',
        'Next event: N',
      ])
      expect(within(help).queryByText(/Single-key shortcuts are off/)).toBeNull()

      press('?', { shiftKey: true })
      await vi.waitFor(() => expect(TestBed.inject(MatDialog).openDialogs).toHaveLength(0))
    })

    it('lists each command once with all the desktop app’s keys, as the platform writes them', async () => {
      await openApp('/day/2026-05-18', { platform: WINDOWS_DESKTOP })

      press('?')

      expect((await helpRows()).slice(0, 10)).toEqual([
        'Collect: Ctrl+R or F5 or R',
        'Filter Activity…: Ctrl+F or /',
        'Timeline: Ctrl+1 or 1',
        'Day: Ctrl+2 or 2',
        'Work week: Ctrl+3 or 3',
        'Week: Ctrl+4 or 4',
        'Settings…: Ctrl+, or ,',
        'Back: Alt+←',
        'Close details or settings: Esc',
        'Keyboard Shortcuts: ?',
      ])
    })
  })
})
