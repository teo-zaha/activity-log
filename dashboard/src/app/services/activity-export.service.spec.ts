import { HttpErrorResponse } from '@angular/common/http'
import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { activityLog, MockBackendTransport, provideMockBackend } from '../testing/activity-fixtures'
import { ActivityExportService } from './activity-export.service'

const range = { start: '2026-05-16T22:00:00Z', end: '2026-05-18T22:00:00Z' }

function setup(backend = new MockBackendTransport()): ActivityExportService {
  TestBed.configureTestingModule({ providers: [provideMockBackend(backend)] })
  return TestBed.inject(ActivityExportService)
}

describe('ActivityExportService', () => {
  it('sends the shown entries, the range and the browser time zone for the backend to render', async () => {
    const backend = new MockBackendTransport()
    const service = setup(backend)

    await service.export('markdown', activityLog.entries, range)

    expect(backend.exports).toEqual([
      {
        format: 'markdown',
        ...range,
        timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        entries: activityLog.entries,
      },
    ])
  })

  it("reports the backend's reason when it rejects an export", async () => {
    const backend = new MockBackendTransport()
    backend.saveExport = (): Promise<void> =>
      Promise.reject(
        new HttpErrorResponse({
          status: 400,
          error: { error: 'Unknown time zone "Mars/Olympus"' },
        })
      )
    const service = setup(backend)

    await expect(service.export('json', [], range)).rejects.toThrow(
      'Export failed: Unknown time zone "Mars/Olympus"'
    )
  })
})
