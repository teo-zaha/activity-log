import { provideLocationMocks } from '@angular/common/testing'
import type { EnvironmentProviders, Provider } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { createEvent, fireEvent, render, screen } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import { AppComponent } from './app.component'
import { provideAppRouter } from './app.routes'
import { ActivityDataService } from './services/activity-data.service'
import { RuntimeEnvironmentService } from './services/backend/runtime-environment.service'
import { ACTIVITY_POLL_ATTEMPTS } from './services/runtime-state/polling-state-source'
import { MockBackendTransport, provideMockBackend } from './testing/activity-fixtures'
import { provideDashboardDates } from './utils/date-adapter'
import { BackendFailure } from './services/backend/backend-failure'

/** The shell in a browser, whatever globals other specs left: no desktop window to ask. */
function shell(backend: MockBackendTransport): (Provider | EnvironmentProviders)[] {
  return [
    provideMockBackend(backend),
    provideAppRouter(),
    provideLocationMocks(),
    provideDashboardDates(),
    { provide: ACTIVITY_POLL_ATTEMPTS, useValue: 1 },
    { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => false } },
  ]
}

describe('AppComponent', () => {
  it('offers Retry for a backend it cannot reach, also with no sources set up', async () => {
    const backend = new MockBackendTransport()
    backend.providersResponse = { currentOs: 'macos', providers: [] }
    backend.activityResponses = [null]
    const { fixture } = await render(AppComponent, { providers: [...shell(backend)] })
    const data = TestBed.inject(ActivityDataService)
    await data.initLoad()
    await fixture.whenStable()
    expect(data.hasNoSources()).toBe(true)

    backend.stateError = new BackendFailure('unreachable', 'activity inaccessible')
    await data.retryCollection()
    await fixture.whenStable()

    expect(screen.getByRole('alert').textContent).toContain('activity inaccessible')
    expect(screen.getByRole('button', { name: 'Retry' })).toBeTruthy()
  })

  it('skips to the main content in place, keeping the URL', async () => {
    const { fixture } = await render(AppComponent, {
      providers: [...shell(new MockBackendTransport())],
    })
    const router = TestBed.inject(Router)
    await router.navigateByUrl('/day/2026-05-18?q=standup')
    await fixture.whenStable()

    const link = screen.getByRole('link', { name: 'Skip to activity' })
    const click = createEvent.click(link)
    fireEvent(link, click)

    expect(click.defaultPrevented).toBe(true)
    expect(document.activeElement).toBe(screen.getByRole('main'))
    await fixture.whenStable()
    expect(router.url).toBe('/day/2026-05-18?q=standup')
  })
})
