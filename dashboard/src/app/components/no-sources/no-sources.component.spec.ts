import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it, vi } from 'vitest'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'
import { SettingsDialogLauncher } from '../settings-dialog/settings-dialog-launcher.service'
import { NoSourcesComponent } from './no-sources.component'

async function setup(desktop: boolean): Promise<ReturnType<typeof vi.fn>> {
  const open = vi.fn()
  await render(NoSourcesComponent, {
    providers: [
      { provide: SettingsDialogLauncher, useValue: { open } },
      { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => desktop } },
    ],
  })
  return open
}

describe('NoSourcesComponent', () => {
  it('opens Add source in the settings', async () => {
    const open = await setup(true)

    await userEvent.click(screen.getByRole('button', { name: 'Add source' }))

    expect(open).toHaveBeenCalledWith('add')
  })

  it('points to the desktop app and the CLI in the browser', async () => {
    await setup(false)

    expect(screen.queryByRole('button', { name: 'Add source' })).toBeNull()
    expect(screen.getByText(/activity-log sources add/)).toBeTruthy()
  })
})
