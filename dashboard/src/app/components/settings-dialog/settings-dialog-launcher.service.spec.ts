import { signal, type Type } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { describe, expect, it, vi } from 'vitest'
import { ActivityService } from '../../services/activity.service'
import { SettingsDialogLauncher } from './settings-dialog-launcher.service'
import type { SettingsDialogOpener } from './settings-dialog-opener.service'
import { SettingsDialogComponent } from './settings-dialog.component'

function setup(launcherClass: Type<SettingsDialogLauncher> = SettingsDialogLauncher): {
  open: ReturnType<typeof vi.fn>
  error: ReturnType<typeof signal<string | null>>
  launcher: SettingsDialogLauncher
} {
  const open = vi.fn()
  const error = signal<string | null>(null)
  TestBed.configureTestingModule({
    providers: [
      { provide: SettingsDialogLauncher, useClass: launcherClass },
      { provide: MatDialog, useValue: { open } },
      { provide: ActivityService, useValue: { error } },
    ],
  })
  return { open, error, launcher: TestBed.inject(SettingsDialogLauncher) }
}

describe('SettingsDialogLauncher', () => {
  it('opens the settings dialog once the lazy chunk has loaded', async () => {
    const { open, launcher } = setup()

    launcher.open()

    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))
    expect(open).toHaveBeenCalledWith(
      SettingsDialogComponent,
      expect.objectContaining({ data: { initialSelection: 'general' }, ariaLabel: 'Settings' })
    )
  })

  it('preselects the requested source', async () => {
    const { open, launcher } = setup()

    launcher.open('jira')

    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))
    expect(open).toHaveBeenCalledWith(
      SettingsDialogComponent,
      expect.objectContaining({ data: { initialSelection: 'source:jira' } })
    )
  })

  it('opens a single dialog for repeated clicks while the chunk is loading', async () => {
    const { open, launcher } = setup()

    launcher.open()
    launcher.open()
    launcher.open()

    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))
    await new Promise(resolve => setTimeout(resolve, 20))
    expect(open).toHaveBeenCalledTimes(1)
  })

  it('reports a failed chunk load and loads again on the next click', async () => {
    let attempts = 0
    class FlakyLauncher extends SettingsDialogLauncher {
      protected override importOpener(): Promise<Type<SettingsDialogOpener>> {
        attempts += 1
        return attempts === 1
          ? Promise.reject(new Error('Failed to fetch dynamically imported module'))
          : super.importOpener()
      }
    }
    const { open, error, launcher } = setup(FlakyLauncher)

    launcher.open()
    await vi.waitFor(() => expect(error()).toBe('Settings could not be loaded. Try again.'))
    expect(open).not.toHaveBeenCalled()

    launcher.open()
    await vi.waitFor(() => expect(open).toHaveBeenCalledTimes(1))
    expect(attempts).toBe(2)
  })
})
