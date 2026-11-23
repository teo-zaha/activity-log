import { signal } from '@angular/core'
import { provideRouter } from '@angular/router'
import { render, screen, within } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import type { ActivityLog } from '../../generated/activity'
import { ActivityExportService } from '../../services/activity-export.service'
import { activityLog } from '../../testing/activity-fixtures'
import { HeaderComponent } from './header.component'

async function setup(state: { loading?: boolean; meta?: ActivityLog['meta'] } = {}): Promise<void> {
  await render(HeaderComponent, {
    providers: [
      provideRouter([]),
      {
        provide: ActivityExportService,
        useValue: {
          canExport: signal(state.meta !== undefined && !state.loading),
        },
      },
    ],
  })
}

describe('HeaderComponent export', () => {
  const exportButtons = (): HTMLElement[] =>
    ['Export Markdown', 'Export JSON'].map(name => screen.getByRole('button', { name }))

  it('is offered for a collected log', async () => {
    await setup({ meta: activityLog.meta })

    expect(exportButtons().map(button => button.hasAttribute('disabled'))).toEqual([false, false])
  })

  it('are the only actions: there is no Share', async () => {
    await setup({ meta: activityLog.meta })

    const actions = screen.getByRole('group', { name: 'Export' })
    expect(within(actions).getAllByRole('button')).toEqual(exportButtons())
    expect(screen.queryByRole('button', { name: /share/i })).toBeNull()
  })

  it('waits until a running collection is complete', async () => {
    await setup({ meta: activityLog.meta, loading: true })

    expect(exportButtons().map(button => button.hasAttribute('disabled'))).toEqual([true, true])
  })
})
