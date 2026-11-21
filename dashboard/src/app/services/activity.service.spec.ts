import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { MockBackendTransport, provideMockBackend } from '../testing/activity-fixtures'
import { ActivityDataService } from './activity-data.service'
import { ActivityExportService } from './activity-export.service'
import { ActivityService } from './activity.service'
import { failure } from './failure'
import { NoticeService } from './notice.service'

describe('ActivityService', () => {
  it('shares the notice and the export state with the services it delegates to', async () => {
    const backend = new MockBackendTransport()
    TestBed.configureTestingModule({ providers: [provideMockBackend(backend)] })
    const activity = TestBed.inject(ActivityService)
    const notices = TestBed.inject(NoticeService)

    // A notice set through the facade is the snackbar's, and the other way round.
    const viewFailed = { origin: 'view', failure: failure('Opening this view failed') } as const
    activity.notice.set(viewFailed)
    expect(notices.current()).toBe(viewFailed)
    notices.clear('view')
    expect(activity.notice()).toBeNull()

    expect(activity.canExport).toBe(TestBed.inject(ActivityExportService).canExport)
    await TestBed.inject(ActivityDataService).initLoad()
    await activity.exportJson()
    expect(backend.exports.map(request => request.format)).toEqual(['json'])
  })
})
