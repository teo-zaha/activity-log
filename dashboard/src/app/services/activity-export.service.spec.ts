import { TestBed } from '@angular/core/testing'
import type { ExportOutcome } from '../generated/activity'
import { describe, expect, it } from 'vitest'
import { activityLog, MockBackendTransport, provideMockBackend } from '../testing/activity-fixtures'
import { ActivityExportService } from './activity-export.service'
import { BackendFailure } from './backend/backend-failure'

const range = { start: '2026-05-16T22:00:00Z', end: '2026-05-18T22:00:00Z' }

function setup(backend = new MockBackendTransport()): ActivityExportService {
  TestBed.configureTestingModule({ providers: [provideMockBackend(backend)] })
  return TestBed.inject(ActivityExportService)
}

describe('ActivityExportService', () => {
  it('sends the shown entries, the range and the browser time zone for the backend to render', async () => {
    const backend = new MockBackendTransport()
    const service = setup(backend)

    await expect(service.export('markdown', activityLog.entries, range)).resolves.toBe('saved')

    expect(backend.exports).toEqual([
      {
        format: 'markdown',
        ...range,
        timeZone: Intl.DateTimeFormat().resolvedOptions().timeZone,
        entries: activityLog.entries,
      },
    ])
  })

  it('rejects with the backend’s failure as it is, kind and all', async () => {
    const backend = new MockBackendTransport()
    const refused = new BackendFailure('invalidRequest', 'Unknown time zone "Mars/Olympus"')
    backend.saveExport = (): Promise<ExportOutcome> => Promise.reject(refused)
    const service = setup(backend)

    await expect(service.export('json', [], range)).rejects.toBe(refused)
  })
})
