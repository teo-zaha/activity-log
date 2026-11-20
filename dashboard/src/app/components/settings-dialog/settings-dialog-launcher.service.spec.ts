import { Location } from '@angular/common'
import { provideLocationMocks, SpyLocation } from '@angular/common/testing'
import { ApplicationRef, signal, type Type } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { Router } from '@angular/router'
import { Subject } from 'rxjs'
import { describe, expect, it, vi } from 'vitest'
import { provideAppRouter } from '../../app.routes'
import { ActivityService } from '../../services/activity.service'
import type { Notice } from '../../services/failure'
import { SettingsDialogLauncher } from './settings-dialog-launcher.service'
import type { SettingsDialogOpener } from './settings-dialog-opener.service'
import { SettingsDialogComponent } from './settings-dialog.component'

async function setup(
  launcherClass: Type<SettingsDialogLauncher> = SettingsDialogLauncher
): Promise<{
  open: ReturnType<typeof vi.fn>
  close: ReturnType<typeof vi.fn<() => void>>
  notice: ReturnType<typeof signal<Notice | null>>
  launcher: SettingsDialogLauncher
  router: Router
  location: SpyLocation
}> {
  const closed = new Subject<void>()
  const close = vi.fn<() => void>(() => closed.next())
  const open = vi.fn(() => ({ afterClosed: (): Subject<void> => closed, close }))
  const notice = signal<Notice | null>(null)
  TestBed.configureTestingModule({
    providers: [
      provideAppRouter(),
      provideLocationMocks(),
      { provide: SettingsDialogLauncher, useClass: launcherClass },
      { provide: MatDialog, useValue: { open } },
      { provide: ActivityService, useValue: { notice } },
    ],
  })
  const router = TestBed.inject(Router)
  const location = TestBed.inject(Location)
  if (!(location instanceof SpyLocation)) throw new Error('location mocks are provided')
  router.initialNavigation()
  await router.navigateByUrl('/timeline')
  location.urlChanges = []
  const launcher = TestBed.inject(SettingsDialogLauncher)
  launcher.followUrl()
  return { open, close, notice, launcher, router, location }
}

describe('SettingsDialogLauncher', () => {
  it('opens the settings as a URL state once the lazy chunk has loaded', async () => {
    const { open, launcher, router } = await setup()

    launcher.open()

    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))
    expect(router.url).toBe('/timeline?settings=general')
    expect(open).toHaveBeenCalledWith(
      SettingsDialogComponent,
      // Going back inside the settings is a navigation the dialog must stay open for.
      expect.objectContaining({ ariaLabel: 'Settings', closeOnNavigation: false })
    )
  })

  it('names the requested source or Add source in the URL', async () => {
    const { launcher, router } = await setup()

    launcher.open('jira/default')
    await vi.waitFor(() => expect(router.url).toBe('/timeline?settings=jira%2Fdefault'))

    launcher.open('add')
    await vi.waitFor(() => expect(router.url).toBe('/timeline?settings=add'))
  })

  it('opens the dialog for a URL that names a section', async () => {
    const { open, router } = await setup()

    await router.navigateByUrl('/day/2026-11-17?settings=logs')

    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))
  })

  it('goes back past the settings when the dialog is closed', async () => {
    const { open, close, launcher, router, location } = await setup()
    launcher.open('add')
    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))

    close()

    await vi.waitFor(() => expect(router.url).toBe('/timeline'))
    expect(location.urlChanges).toEqual(['/timeline?settings=add'])
  })

  it('closes the dialog when Back leaves the settings URL', async () => {
    const { open, close, launcher, router, location } = await setup()
    launcher.open()
    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))

    location.back()

    await vi.waitFor(() => expect(close).toHaveBeenCalledTimes(1))
    expect(router.url).toBe('/timeline')
  })

  it('opens a single dialog for repeated clicks while the chunk is loading', async () => {
    const { open, launcher } = await setup()

    launcher.open()
    launcher.open()
    launcher.open()

    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))
    await new Promise(resolve => setTimeout(resolve, 20))
    expect(open).toHaveBeenCalledTimes(1)
  })

  it('reports a failed chunk load, drops the parameter and loads again on the next click', async () => {
    let attempts = 0
    class FlakyLauncher extends SettingsDialogLauncher {
      protected override importOpener(): Promise<Type<SettingsDialogOpener>> {
        attempts += 1
        return attempts === 1
          ? Promise.reject(new Error('Failed to fetch dynamically imported module'))
          : super.importOpener()
      }
    }
    const { open, notice, launcher, router } = await setup(FlakyLauncher)

    launcher.open()
    await vi.waitFor(() =>
      expect(notice()).toMatchObject({
        origin: 'settings',
        failure: {
          message: 'Loading the settings failed: Failed to fetch dynamically imported module',
        },
      })
    )
    await vi.waitFor(() => expect(router.url).toBe('/timeline'))
    expect(open).not.toHaveBeenCalled()

    launcher.open()
    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))
    expect(attempts).toBe(2)
  })

  it('does not open the dialog after the URL dropped the settings while its code loaded', async () => {
    let finishLoading = (): void => undefined
    class SlowLauncher extends SettingsDialogLauncher {
      protected override importOpener(): Promise<Type<SettingsDialogOpener>> {
        const load = super.importOpener()
        return new Promise(resolve => (finishLoading = (): void => resolve(load)))
      }
    }
    const { open, launcher, router } = await setup(SlowLauncher)
    launcher.open()
    await vi.waitFor(() => expect(router.url).toBe('/timeline?settings=general'))
    await router.navigateByUrl('/timeline')
    finishLoading()
    await new Promise(resolve => setTimeout(resolve, 20))
    expect(open).not.toHaveBeenCalled()
  })

  it('keeps the phone page open when the dialog code fails to load after the window narrowed', async () => {
    let failLoading = (): void => undefined
    class FailingLauncher extends SettingsDialogLauncher {
      protected override importOpener(): Promise<Type<SettingsDialogOpener>> {
        return new Promise((_, reject) => {
          failLoading = (): void => reject(new Error('Failed to fetch dynamically imported module'))
        })
      }
    }
    const phone = mockPhone(false)
    try {
      const { open, notice, launcher, router } = await setup(FailingLauncher)
      const app = TestBed.inject(ApplicationRef)
      launcher.open('add')
      await app.whenStable()
      expect(router.url).toBe('/timeline?settings=add')

      phone.set(true)
      await app.whenStable()
      expect(launcher.pageOpen()).toBe(true)
      failLoading()
      await app.whenStable()

      expect(launcher.pageOpen()).toBe(true)
      expect(router.url).toBe('/timeline?settings=add')
      expect(notice()).toBeNull()
      expect(open).not.toHaveBeenCalled()
    } finally {
      phone.restore()
    }
  })

  it('shows the settings as the Settings tab on a phone', async () => {
    const phone = mockPhone(true)
    try {
      const { open, launcher, router } = await setup()
      // The media query answers after a tick.
      await new Promise(resolve => setTimeout(resolve))

      launcher.open()
      await vi.waitFor(() => expect(launcher.pageOpen()).toBe(true))
      expect(router.url).toBe('/timeline?settings=home')
      expect(open).not.toHaveBeenCalled()

      await router.navigateByUrl('/timeline')
      await vi.waitFor(() => expect(launcher.pageOpen()).toBe(false))
    } finally {
      phone.restore()
    }
  })

  it('moves open settings between the dialog and the page as the window crosses the phone width', async () => {
    const phone = mockPhone(false)
    try {
      const { open, close, launcher, router } = await setup()
      launcher.open('add')
      await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))

      phone.set(true)
      await vi.waitFor(() => expect(launcher.pageOpen()).toBe(true))
      expect(close).toHaveBeenCalledTimes(1)
      expect(router.url).toBe('/timeline?settings=add')

      phone.set(false)
      await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(2))
      expect(launcher.pageOpen()).toBe(false)
      expect(router.url).toBe('/timeline?settings=add')
    } finally {
      phone.restore()
    }
  })
})

/** Answers the phone breakpoint's media query with `matches`, which `set` changes. */
function mockPhone(matches: boolean): { set(matches: boolean): void; restore(): void } {
  const list = Object.assign(new EventTarget(), { matches })
  const realMatchMedia = window.matchMedia
  window.matchMedia = (query: string): MediaQueryList =>
    query === '(max-width: 640px)'
      ? {
          ...realMatchMedia(query),
          matches: list.matches,
          media: query,
          addEventListener: list.addEventListener.bind(list),
          removeEventListener: list.removeEventListener.bind(list),
        }
      : realMatchMedia(query)
  return {
    set(next: boolean): void {
      list.matches = next
      list.dispatchEvent(Object.assign(new Event('change'), { matches: next }))
    },
    restore(): void {
      window.matchMedia = realMatchMedia
    },
  }
}
