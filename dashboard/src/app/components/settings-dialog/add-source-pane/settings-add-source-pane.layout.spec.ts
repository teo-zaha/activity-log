/// <reference types="@vitest/browser-playwright" />
// Add source in a real browser (`ng test --configuration layout`, Chromium): scrolled to its end,
// the Coming soon list keeps the room below it that the other settings' panes keep, in the dialog
// and on a phone's Settings tab, which only a layout engine can tell.
import { TestBed } from '@angular/core/testing'
import { MatDialog } from '@angular/material/dialog'
import { firstValueFrom } from 'rxjs'
import { afterEach, describe, expect, it } from 'vitest'
import { page } from 'vitest/browser'
import type {
  ProviderConfigSnapshotDto,
  ProviderKindDto,
  Source,
} from '../../../generated/activity'
import { RuntimeEnvironmentService } from '../../../services/backend/runtime-environment.service'
import { ConnectionSettings } from '../../../services/connection-settings.service'
import { openApp } from '../../../testing/app'
import { waitForLayout } from '../../../testing/layout'
import { SettingsAddSourcePaneComponent } from './settings-add-source-pane.component'

/** The room below a pane's last row: General's and Logs' `padding-bottom`. */
const PANE_END = 24

const kind = (source: Source, label: string): ProviderKindDto => ({
  source,
  label,
  description: `What ${label} collects`,
  multiple: true,
  supportedOs: ['windows', 'macos', 'linux'],
  supported: true,
  wslDistros: [],
})

const SNAPSHOT: ProviderConfigSnapshotDto = {
  revision: 'r1',
  kinds: [
    kind('git', 'Git'),
    kind('gitlab', 'GitLab'),
    kind('github', 'GitHub'),
    kind('jira', 'Jira'),
    kind('outlook', 'Outlook'),
    kind('system', 'System'),
    kind('calendar', 'Calendar'),
    kind('figma', 'Figma'),
  ],
  connections: [],
}

/** Opens Add source as the desktop app shows it, its tiles read from `SNAPSHOT`. */
async function openAddSource(role: 'dialog' | 'region'): Promise<HTMLElement> {
  // Only the pane takes the page for the desktop app; the shell keeps the browser's transport.
  TestBed.overrideComponent(SettingsAddSourcePaneComponent, {
    add: {
      providers: [
        { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => true } },
        {
          provide: ConnectionSettings,
          useValue: {
            loadProviderConfig: (): Promise<ProviderConfigSnapshotDto> => Promise.resolve(SNAPSHOT),
          },
        },
      ],
    },
  })
  const { settle } = await openApp('/timeline?settings=add')
  const settings = page.getByRole(role, { name: 'Settings' })
  await expect
    .element(settings.getByRole('heading', { name: 'Add source', level: 2 }))
    .toBeVisible()
  const scroller = settings.element().querySelector<HTMLElement>('.settings-scroll')
  if (!scroller) throw new Error('the settings page scrolls')
  await expect
    .poll(() => scroller.querySelectorAll('.upcoming-grid li').length, { timeout: 10_000 })
    .toBeGreaterThan(0)
  if (role === 'dialog') {
    const [dialog] = TestBed.inject(MatDialog).openDialogs
    if (!dialog) throw new Error('the settings dialog opened')
    await firstValueFrom(dialog.afterOpened())
  }
  await settle()
  await waitForLayout()
  return scroller
}

/** How far the last Coming soon tile ends above the page's visible bottom, scrolled to the end. */
function roomBelowLastTile(scroller: HTMLElement): number {
  expect(scroller.scrollHeight).toBeGreaterThan(scroller.clientHeight)
  scroller.scrollTo({ top: scroller.scrollHeight, behavior: 'instant' })
  const tiles = scroller.querySelectorAll('.upcoming-grid li')
  const last = tiles.item(tiles.length - 1).getBoundingClientRect()
  return scroller.getBoundingClientRect().bottom - last.bottom
}

describe('Add source in a browser', () => {
  afterEach(() => {
    localStorage.clear()
  })

  it('ends with room below Coming soon in the dialog, scrolled to its end', async () => {
    // Short enough that the dialog's page scrolls.
    await page.viewport(1280, 560)
    const scroller = await openAddSource('dialog')

    expect(roomBelowLastTile(scroller)).toBeGreaterThanOrEqual(PANE_END - 0.5)
  })

  it('ends with room below Coming soon on a phone’s Settings tab', async () => {
    await page.viewport(390, 700)
    const scroller = await openAddSource('region')

    expect(roomBelowLastTile(scroller)).toBeGreaterThanOrEqual(PANE_END - 0.5)
  })
})
