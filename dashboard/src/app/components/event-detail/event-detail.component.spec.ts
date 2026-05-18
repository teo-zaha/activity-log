import { signal } from '@angular/core'
import { fireEvent, render, screen } from '@testing-library/angular'
import { describe, expect, it } from 'vitest'
import { createEntryAdapter } from '../../providers/provider-adapter'
import { ActivityService } from '../../services/activity.service'
import { activityLog } from '../../testing/activity-fixtures'
import { EventDetailComponent } from './event-detail.component'

describe('EventDetailComponent', () => {
  it('renders selected entry details as a named modal dialog', async () => {
    const entry = createEntryAdapter(activityLog.entries[0])
    const activity = {
      selectedEntry: signal(entry),
      selectedEntries: signal(null),
    }

    await render(EventDetailComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    expect(screen.getByRole('dialog', { name: entry.title }).getAttribute('aria-modal')).toBe(
      'true'
    )
    expect(screen.getAllByRole('button', { name: /close event details/i })).toHaveLength(2)
  })

  it('closes on Escape', async () => {
    const entry = createEntryAdapter(activityLog.entries[0])
    const selectedEntry = signal(entry)
    const activity = {
      selectedEntry,
      selectedEntries: signal(null),
    }

    await render(EventDetailComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    fireEvent.keyDown(screen.getByRole('dialog', { name: entry.title }), { key: 'Escape' })

    expect(selectedEntry()).toBeNull()
  })
})
