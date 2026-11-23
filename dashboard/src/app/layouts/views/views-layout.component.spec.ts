import { provideLocationMocks } from '@angular/common/testing'
import { TestBed } from '@angular/core/testing'
import { provideRouter } from '@angular/router'
import { render } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import { ActivityDataService } from '../../services/activity-data.service'
import { RuntimeEnvironmentService } from '../../services/backend/runtime-environment.service'
import { RECONNECT_DELAYS_MS } from '../../services/runtime-state/push-state-source'
import { MockBackendTransport, provideMockBackend } from '../../testing/activity-fixtures'
import { ShellSlots } from '../shell/shell-slots'
import { ViewsLayoutComponent } from './views-layout.component'

async function setup(backend = new MockBackendTransport()): Promise<{
  slots: ShellSlots
  container: Element
  destroy: () => void
}> {
  const { container, fixture } = await render(ViewsLayoutComponent, {
    providers: [
      ShellSlots,
      provideMockBackend(backend),
      provideRouter([]),
      provideLocationMocks(),
      { provide: RECONNECT_DELAYS_MS, useValue: [] },
      { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => false } },
    ],
  })
  await TestBed.inject(ActivityDataService).initLoad()
  await fixture.whenStable()
  return { slots: TestBed.inject(ShellSlots), container, destroy: () => fixture.destroy() }
}

describe('ViewsLayoutComponent', () => {
  it('fills the shell’s header and sidebar for as long as it lives', async () => {
    const { slots, destroy } = await setup()
    expect(slots.template('header')()).not.toBeNull()
    expect(slots.template('sidebar')()).not.toBeNull()

    destroy()

    expect(slots.template('header')()).toBeNull()
    expect(slots.template('sidebar')()).toBeNull()
  })

  it('shows what to do without sources instead of a view, and the collection status with either', async () => {
    const backend = new MockBackendTransport()
    backend.providersResponse = {
      currentOs: 'macos',
      secretStore: { store: 'os', state: 'ready' },
      providers: [],
    }
    const { container } = await setup(backend)

    expect(container.querySelector('app-no-sources')).not.toBeNull()
    expect(container.querySelector('router-outlet')).toBeNull()
    expect(container.querySelector('app-collection-status')).not.toBeNull()
  })

  it('shows the view’s outlet once there are sources', async () => {
    const { container } = await setup()

    expect(container.querySelector('router-outlet')).not.toBeNull()
    expect(container.querySelector('app-no-sources')).toBeNull()
  })
})
