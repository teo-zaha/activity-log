import { TestBed } from '@angular/core/testing'
import { describe, expect, it, vi } from 'vitest'
import { Router } from '@angular/router'
import type { ActivityEntry, ExportOutcome } from '../generated/activity'
import { provideAppRouter } from '../app.routes'
import {
  activityLog,
  MockBackendTransport,
  providerInfo,
  provideMockBackend,
} from '../testing/activity-fixtures'
import { ActivityDataService } from './activity-data.service'
import { ActivityExportService } from './activity-export.service'
import { ActivityService } from './activity.service'
import { BackendFailure } from './backend/backend-failure'
import { ToastService } from './toast.service'

function setup(exportFn: ActivityExportService['export']): ActivityService {
  TestBed.configureTestingModule({
    providers: [
      provideMockBackend(new MockBackendTransport()),
      { provide: ActivityExportService, useValue: { export: exportFn } },
    ],
  })
  return TestBed.inject(ActivityService)
}

describe('ActivityService exports', () => {
  it('exports the shown entries over the chosen local days', async () => {
    const exportFn = vi.fn<ActivityExportService['export']>(() => Promise.resolve('saved'))
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

  it('shows why the backend refused an export, and keeps its kind', async () => {
    const refused = new BackendFailure('invalidRequest', 'Unknown time zone "X"')
    const service = setup(() => Promise.reject(refused))
    await TestBed.inject(ActivityDataService).initLoad()

    await service.exportJson()

    expect(service.notice()).toEqual({
      origin: 'export',
      failure: { kind: 'backend', message: 'Export failed: Unknown time zone "X"', cause: refused },
    })
  })

  it('confirms an export that was saved, and says nothing when the dialog was closed', async () => {
    const outcomes: ExportOutcome[] = ['saved', 'cancelled']
    const service = setup(() => Promise.resolve(outcomes.shift() ?? 'cancelled'))
    const success = vi.spyOn(TestBed.inject(ToastService), 'success')
    await TestBed.inject(ActivityDataService).initLoad()

    await service.exportJson()
    expect(success).toHaveBeenCalledExactlyOnceWith('Exported')

    await service.exportJson()
    expect(success).toHaveBeenCalledOnce()
    expect(service.notice()).toBeNull()
  })

  it('has nothing to export before the first collection', async () => {
    const exportFn = vi.fn<ActivityExportService['export']>(() => Promise.resolve('saved'))
    const service = setup(exportFn)

    await service.exportJson()

    expect(exportFn).not.toHaveBeenCalled()
    expect(service.notice()).toEqual({
      origin: 'export',
      failure: { kind: 'other', message: 'Export failed: nothing has been collected yet' },
    })
  })

  it('exports what a calendar view draws, a meeting that runs into the range included', async () => {
    const backend = new MockBackendTransport()
    const overnight: ActivityEntry = {
      source: 'calendar',
      connection: 'team',
      id: 'night-shift',
      // 16 May 22:00 to 17 May 02:00 local; the range starts on the 17th.
      timestamp: new Date(2026, 4, 16, 22).toISOString(),
      title: 'Night shift',
      durationMinutes: 240,
      meetingWeight: 'busy',
    }
    backend.providersResponse = {
      currentOs: 'linux',
      providers: [
        ...providerInfo,
        { ...providerInfo[0]!, connection: { source: 'calendar', name: 'team' }, label: 'Team' },
      ],
    }
    backend.activityResponses = []
    backend.activityResponse = { ...activityLog, entries: [...activityLog.entries, overnight] }
    const exportFn = vi.fn<ActivityExportService['export']>(() => Promise.resolve('saved'))
    TestBed.configureTestingModule({
      providers: [
        provideMockBackend(backend),
        provideAppRouter(),
        { provide: ActivityExportService, useValue: { export: exportFn } },
      ],
    })
    const service = TestBed.inject(ActivityService)
    await service.initLoad()
    await TestBed.inject(Router).navigateByUrl('/week/2026-05-18')

    await service.exportMarkdown()

    const exported = exportFn.mock.calls[0]?.[1] ?? []
    expect(exported.map(entry => entry.id)).toContain('night-shift')
    expect(exported).toHaveLength(3)

    await TestBed.inject(Router).navigateByUrl('/timeline')
    await service.exportMarkdown()
    // The timeline lists entries by the day they start: the night shift is the 16th's.
    expect(exportFn.mock.calls[1]?.[1].map(entry => entry.id)).not.toContain('night-shift')
  })
})
