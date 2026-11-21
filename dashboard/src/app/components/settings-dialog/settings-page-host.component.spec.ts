import { provideLocationMocks } from '@angular/common/testing'
import { type Injector, signal } from '@angular/core'
import { DeferBlockBehavior } from '@angular/core/testing'
import { Router } from '@angular/router'
import { fireEvent, render, screen } from '@testing-library/angular'
import { describe, expect, it, vi } from 'vitest'
import { provideAppRouter } from '../../app.routes'
import { ActivityDataService } from '../../services/activity-data.service'
import { failure } from '../../services/failure'
import { NoticeService } from '../../services/notice.service'
import { MockBackendTransport, provideMockBackend } from '../../testing/activity-fixtures'
import { SettingsDialogLauncher } from './settings-dialog-launcher.service'
import { LOGS_POLL_INTERVAL_MS } from './logs-pane/settings-logs-pane.component'
import { SettingsPageHostComponent } from './settings-page-host.component'
import { SourceDrafts } from './source-pane/source-drafts.service'
import { mockPhone } from '../../testing/media'
import { holdMotion } from '../../testing/motion'
import { APP_RENDER_TEST } from '../../testing/timeouts'

async function openSettings(
  url: string
): Promise<{ router: Router; settle: () => Promise<void>; injector: Injector }> {
  const backend = new MockBackendTransport()
  backend.logs = [
    { id: 1, timestamp: '2026-11-17T08:00:00Z', level: 'info', scope: 'git', message: 'Scanned' },
  ]
  const { fixture } = await render(SettingsPageHostComponent, {
    deferBlockBehavior: DeferBlockBehavior.Playthrough,
    // As a browser does: the pages' enter and leave callbacks run.
    configureTestBed: testBed => testBed.configureTestingModule({ animationsEnabled: true }),
    providers: [
      provideAppRouter(),
      provideLocationMocks(),
      provideMockBackend(backend),
      { provide: LOGS_POLL_INTERVAL_MS, useValue: 60_000 },
      {
        provide: SettingsDialogLauncher,
        useValue: { pageOpen: signal(true), dialogOpen: signal(false) },
      },
    ],
  })
  const router = fixture.debugElement.injector.get(Router)
  await router.navigateByUrl(url)
  await fixture.whenStable()
  return { router, settle: () => fixture.whenStable(), injector: fixture.debugElement.injector }
}

async function openLogs(): Promise<{ router: Router; settle: () => Promise<void> }> {
  const opened = await openSettings('/timeline?settings=logs')
  await screen.findByText('Scanned')
  return opened
}

describe('SettingsPageHostComponent', APP_RENDER_TEST, () => {
  it('keeps focus in the settings when the focused control is disabled', async () => {
    const { settle } = await openLogs()
    const clear = screen.getByRole('button', { name: /Clear/ })
    clear.focus()

    fireEvent.click(clear)
    await vi.waitFor(() => expect(clear.hasAttribute('disabled')).toBe(true))
    await settle()

    expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Logs' }))
  })

  it('closes on Escape with focus on nothing, unless a dialog over it handled it', async () => {
    const { router } = await openLogs()
    ;(document.activeElement as HTMLElement | null)?.blur()

    const handled = new KeyboardEvent('keydown', { key: 'Escape', bubbles: true, cancelable: true })
    handled.preventDefault()
    document.body.dispatchEvent(handled)
    await new Promise(resolve => setTimeout(resolve))
    expect(router.url).toBe('/timeline?settings=logs')

    fireEvent.keyDown(document.body, { key: 'Escape' })
    await vi.waitFor(() => expect(router.url).toBe('/timeline'))
  })

  it('slides the list out as a section comes: the list is inert until it has gone, the section’s title has focus', async () => {
    const phone = mockPhone(true)
    const motion = holdMotion()
    try {
      const { router, settle } = await openSettings('/timeline?settings=home')
      await screen.findByRole('heading', { name: 'Settings' })
      const list = document.querySelector('app-settings-nav')

      await router.navigateByUrl('/timeline?settings=logs')
      await settle()

      const section = screen.getByRole('heading', { name: 'Logs' }).closest('section')
      expect(list?.isConnected).toBe(true)
      expect(list?.hasAttribute('inert')).toBe(true)
      expect([...(list?.classList ?? [])]).toEqual(
        expect.arrayContaining(['page-leave', 'motion-page-in'])
      )
      expect([...(section?.classList ?? [])]).toEqual(
        expect.arrayContaining(['page-enter', 'motion-page-in'])
      )
      await vi.waitFor(() =>
        expect(document.activeElement).toBe(screen.getByRole('heading', { name: 'Logs' }))
      )

      motion.finish()
      await vi.waitFor(() => expect(list?.isConnected).toBe(false))
      expect(section?.classList.contains('page-enter')).toBe(false)
    } finally {
      motion.restore()
      phone.restore()
    }
  })

  it('says so for a source that is not set up, once the sources are in, with a way on', async () => {
    const { router, injector } = await openSettings('/timeline?settings=jira%2Fgone')
    expect(screen.getByRole('status').textContent).toContain('Loading sources')
    expect(screen.queryByText('Theme')).toBeNull()

    await injector.get(ActivityDataService).initLoad()

    expect(await screen.findByRole('heading', { name: 'Source not found' })).toBeTruthy()
    expect(screen.getByText('This source is not set up')).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Show General' }))
    await vi.waitFor(() => expect(router.url).toBe('/timeline?settings=general'))
  })

  it('keeps the unsaved changes of a source another program removed reachable', async () => {
    const { router, injector } = await openSettings('/timeline?settings=jira%2Fgone')
    const drafts = injector.get(SourceDrafts)
    drafts.set('jira/gone', {
      basedOn: 'r1',
      label: 'Gone Jira',
      values: { JIRA_URL: { kind: 'replace', value: 'https://jira.example.org' } },
      lists: {},
    })

    await injector.get(ActivityDataService).initLoad()

    expect(await screen.findByText(/Another program removed Gone Jira/)).toBeTruthy()
    expect(screen.getByRole('button', { name: 'Add it again' })).toBeTruthy()
    fireEvent.click(screen.getByRole('button', { name: 'Discard my changes' }))
    await vi.waitFor(() => expect(router.url).toBe('/timeline?settings=general'))
    expect(drafts.get('jira/gone')).toBeUndefined()
  })

  it('keeps waiting for the sources while a notice about something else shows', async () => {
    const { injector, settle } = await openSettings('/timeline?settings=jira%2Fgone')

    injector.get(NoticeService).show({
      origin: 'export',
      failure: failure('Exporting failed: the disk is full'),
    })
    await settle()

    expect(screen.getByRole('status').textContent).toContain('Loading sources')
    expect(screen.queryByRole('heading', { name: 'Source not found' })).toBeNull()
  })
})
