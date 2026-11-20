import { provideLocationMocks } from '@angular/common/testing'
import type { EnvironmentProviders, Provider } from '@angular/core'
import { DeferBlockBehavior, TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { render } from '@testing-library/angular'
import { AppComponent } from '../app.component'
import { provideAppRouter } from '../app.routes'
import { LOGS_POLL_INTERVAL_MS } from '../components/settings-dialog/logs-pane/settings-logs-pane.component'
import { RuntimeEnvironmentService } from '../services/backend/runtime-environment.service'
import { RECONNECT_DELAYS_MS } from '../services/runtime-state/push-state-source'
import { SHORTCUT_PLATFORM, type ShortcutPlatform } from '../services/shortcut.service'
import { provideDashboardDates } from '../utils/date-adapter'
import { MockBackendTransport, provideMockBackend } from './activity-fixtures'
import { stubLayoutObservers } from './observers'

/** The app in a browser: the backend, and the services every page uses. */
function app(
  backend: MockBackendTransport,
  platform: ShortcutPlatform
): (Provider | EnvironmentProviders)[] {
  return [
    provideMockBackend(backend),
    provideLocationMocks(),
    provideDashboardDates(),
    { provide: RECONNECT_DELAYS_MS, useValue: [] },
    { provide: LOGS_POLL_INTERVAL_MS, useValue: 60_000 },
    { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => false } },
    // The keys of the platform a spec names; the transport stays the browser's.
    { provide: SHORTCUT_PLATFORM, useValue: platform },
  ]
}

export interface OpenApp {
  readonly router: Router
  readonly settle: () => Promise<void>
  readonly backend: MockBackendTransport
}

/** Renders the app, its shell routed in, and navigates to `url` as its first navigation. */
export async function openApp(
  url: string,
  options: {
    backend?: MockBackendTransport
    router?: Provider | EnvironmentProviders
    /** The browser on macOS unless set. */
    platform?: ShortcutPlatform
  } = {}
): Promise<OpenApp> {
  stubLayoutObservers()
  const backend = options.backend ?? new MockBackendTransport()
  const platform = options.platform ?? { os: 'mac', build: 'browser' }
  const { fixture } = await render(AppComponent, {
    providers: [...app(backend, platform), options.router ?? provideAppRouter()],
    // As a browser does: what comes and goes runs its enter and leave callbacks.
    configureTestBed: testBed =>
      testBed.configureTestingModule({
        animationsEnabled: true,
        deferBlockBehavior: DeferBlockBehavior.Playthrough,
      }),
  })
  const router = TestBed.inject(Router)
  await router.navigateByUrl(url)
  await fixture.whenStable()
  return { router, settle: () => fixture.whenStable(), backend }
}
