import { signal } from '@angular/core'
import { render, screen } from '@testing-library/angular'
import userEvent from '@testing-library/user-event'
import { describe, expect, it } from 'vitest'
import { createEntryAdapter } from '../../providers/provider-adapter'
import { ActivityService } from '../../services/activity.service'
import { activityLog } from '../../testing/activity-fixtures'
import { TimelineComponent } from './timeline.component'

describe('TimelineComponent', () => {
  it('allows keyboard activation of entries', async () => {
    const entry = createEntryAdapter(activityLog.entries[0])
    const selectedEntry = signal(null)
    const activity = {
      isLoading: signal(false),
      groupedByDay: signal([{ date: '2026-05-18', entries: [entry] }]),
      searchQuery: signal(''),
      selectedEntries: signal(null),
      selectedEntry,
    }

    await render(TimelineComponent, {
      providers: [{ provide: ActivityService, useValue: activity }],
    })

    const button = screen.getByRole('button', { name: /open git event/i })
    button.focus()
    expect(button).toBe(document.activeElement)
    await userEvent.keyboard('{Enter}')

    expect(selectedEntry()).toBe(entry)
  })
})
