import { TestBed } from '@angular/core/testing'
import { describe, expect, it, vi } from 'vitest'
import { activityLog, MockBackendTransport, provideMockBackend } from '../testing/activity-fixtures'
import { ACTIVITY_POLL_DELAY_MS, ActivityDataService } from './activity-data.service'
import { ActivityExportService } from './activity-export.service'
import { ActivityService } from './activity.service'

function setup(exportFn: ActivityExportService['export']): ActivityService {
  TestBed.configureTestingModule({
    providers: [
      provideMockBackend(new MockBackendTransport()),
      { provide: ACTIVITY_POLL_DELAY_MS, useValue: 1 },
      { provide: ActivityExportService, useValue: { export: exportFn } },
    ],
  })
  return TestBed.inject(ActivityService)
}

describe('ActivityService exports', () => {
  it('exports the shown entries over the chosen local days', async () => {
    const exportFn = vi.fn<ActivityExportService['export']>(() => Promise.resolve())
    const service = setup(exportFn)
    await TestBed.inject(ActivityDataService).initLoad()

    await service.exportMarkdown()

    expect(exportFn).toHaveBeenCalledWith(
      'markdown',
      service.filteredEntries().map(entry => entry.entry),
      // The loaded range is 17–18 May in local days; their midnights are the fixture's range.
      { start: activityLog.meta.start, end: activityLog.meta.end }
    )
  })

  it('shows why the backend refused an export', async () => {
    const service = setup(() => Promise.reject(new Error('Export failed: Unknown time zone "X"')))
    await TestBed.inject(ActivityDataService).initLoad()

    await service.exportJson()

    expect(service.error()).toBe('Export failed: Unknown time zone "X"')
  })

  it('has nothing to export before the first collection', async () => {
    const exportFn = vi.fn<ActivityExportService['export']>(() => Promise.resolve())
    const service = setup(exportFn)

    await service.exportJson()

    expect(exportFn).not.toHaveBeenCalled()
    expect(service.error()).toBe('Export failed: nothing has been collected yet')
  })
})
