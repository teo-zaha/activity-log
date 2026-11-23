import { TestBed } from '@angular/core/testing'
import { Router } from '@angular/router'
import { describe, expect, it, onTestFinished, vi } from 'vitest'
import type { ActivityEntry, ExportOutcome } from '../generated/activity'
import { provideAppRouter } from '../app.routes'
import {
  activityLog,
  collectingLog,
  MockBackendTransport,
  providerInfo,
  provideMockBackend,
} from '../testing/activity-fixtures'
import { ActivityDataService } from './activity-data.service'
import { ActivityExportService } from './activity-export.service'
import { BackendFailure } from './backend/backend-failure'
import { failure } from './failure'
import { NoticeService } from './notice.service'
import { ToastService } from './toast.service'

function setup(backend = new MockBackendTransport()): ActivityExportService {
  TestBed.configureTestingModule({ providers: [provideMockBackend(backend), provideAppRouter()] })
  return TestBed.inject(ActivityExportService)
}

describe('ActivityExportService', () => {
  it('sends the shown entries over the chosen local days, in the browser’s time zone', async () => {
    const backend = new MockBackendTransport()
    const service = setup(backend)
    await TestBed.inject(ActivityDataService).initLoad()

    await service.exportShown('markdown')

    expect(backend.exports).toEqual([
      {
        format: 'markdown',
        // The loaded range is 17–18 May in local days; their midnights are the fixture's range.
        start: activityLog.meta.start,
        end: activityLog.meta.end,
        timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        entries: activityLog.entries,
      },
    ])
  })

  it('shows why the backend refused an export, and keeps its kind', async () => {
    const backend = new MockBackendTransport()
    const refused = new BackendFailure('invalidRequest', 'Unknown time zone "X"')
    backend.saveExport = (): Promise<ExportOutcome> => Promise.reject(refused)
    const service = setup(backend)
    await TestBed.inject(ActivityDataService).initLoad()

    await service.exportShown('json')

    expect(TestBed.inject(NoticeService).current()).toEqual({
      origin: 'export',
      failure: { kind: 'backend', message: 'Export failed: Unknown time zone "X"', cause: refused },
    })
  })

  it('confirms an export that was saved, and says nothing when the dialog was closed', async () => {
    const backend = new MockBackendTransport()
    const outcomes: ExportOutcome[] = ['saved', 'cancelled']
    backend.saveExport = (): Promise<ExportOutcome> =>
      Promise.resolve(outcomes.shift() ?? 'cancelled')
    const service = setup(backend)
    const success = vi.spyOn(TestBed.inject(ToastService), 'success')
    await TestBed.inject(ActivityDataService).initLoad()

    await service.exportShown('json')
    expect(success).toHaveBeenCalledExactlyOnceWith('Exported')

    await service.exportShown('json')
    expect(success).toHaveBeenCalledOnce()
    expect(TestBed.inject(NoticeService).current()).toBeNull()
  })

  it('clears an export’s failure when it is tried again, and no other notice', async () => {
    const service = setup()
    const notices = TestBed.inject(NoticeService)
    await TestBed.inject(ActivityDataService).initLoad()

    notices.show({ origin: 'export', failure: failure('Export failed') })
    await service.exportShown('json')
    expect(notices.current()).toBeNull()

    const viewFailed = { origin: 'view', failure: failure('Opening this view failed') } as const
    notices.show(viewFailed)
    await service.exportShown('json')
    expect(notices.current()).toBe(viewFailed)
  })

  it('has nothing to export before the first collection', async () => {
    const backend = new MockBackendTransport()
    const service = setup(backend)

    expect(service.canExport()).toBe(false)
    await service.exportShown('json')

    expect(backend.exports).toEqual([])
    expect(TestBed.inject(NoticeService).current()).toEqual({
      origin: 'export',
      failure: { kind: 'other', message: 'Export failed: nothing has been collected yet' },
    })
  })

  it('can export once a log is in, and not while a collection replaces it', async () => {
    vi.useFakeTimers()
    onTestFinished(() => void vi.useRealTimers())
    const backend = new MockBackendTransport()
    const service = setup(backend)
    const data = TestBed.inject(ActivityDataService)
    await data.initLoad()
    expect(service.canExport()).toBe(true)

    backend.collectResponse = collectingLog
    backend.activityResponses = [collectingLog]
    await data.collect('2026-05-17', '2026-05-18')
    expect(data.isLoading()).toBe(true)
    expect(service.canExport()).toBe(false)

    await vi.runAllTimersAsync()
    expect(service.canExport()).toBe(true)
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
      secretStore: { store: 'os', state: 'ready' },
      providers: [
        ...providerInfo,
        { ...providerInfo[0]!, connection: { source: 'calendar', name: 'team' }, label: 'Team' },
      ],
    }
    backend.activityResponses = []
    backend.activityResponse = { ...activityLog, entries: [...activityLog.entries, overnight] }
    const service = setup(backend)
    await TestBed.inject(ActivityDataService).initLoad()
    await TestBed.inject(Router).navigateByUrl('/week/2026-05-18')

    await service.exportShown('markdown')

    const exported = backend.exports[0]?.entries ?? []
    expect(exported.map(entry => entry.id)).toContain('night-shift')
    expect(exported).toHaveLength(3)

    await TestBed.inject(Router).navigateByUrl('/timeline')
    await service.exportShown('markdown')
    // The timeline lists entries by the day they start: the night shift is the 16th's.
    expect(backend.exports[1]?.entries.map(entry => entry.id)).not.toContain('night-shift')
  })
})
