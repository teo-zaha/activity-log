import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { activityLog, MockBackendTransport, provideMockBackend } from '../testing/activity-fixtures'
import { createEntryAdapter } from '../providers/provider-adapter'
import { ActivityExportService } from './activity-export.service'

describe('ActivityExportService', () => {
  it('exports filtered entries as JSON', async () => {
    const backend = new MockBackendTransport()
    TestBed.configureTestingModule({
      providers: [provideMockBackend(backend)],
    })
    const service = TestBed.inject(ActivityExportService)
    const entries = activityLog.entries.map(createEntryAdapter)

    await service.exportJson(activityLog.meta, entries, '2026-05-17', '2026-05-18')

    expect(backend.exports[0].fileName).toBe('activity-2026-05-17-2026-05-18.json')
    expect(JSON.parse(backend.exports[0].content)).toMatchObject({ entries: activityLog.entries })
  })

  it('exports grouped markdown summaries', async () => {
    const backend = new MockBackendTransport()
    TestBed.configureTestingModule({
      providers: [provideMockBackend(backend)],
    })
    const service = TestBed.inject(ActivityExportService)
    const entries = activityLog.entries.map(createEntryAdapter)

    await service.exportMarkdown('2026-05-17', '2026-05-18', entries, [
      { date: '2026-05-18', entries },
    ])

    expect(backend.exports[0].fileName).toBe('activity-2026-05-17-2026-05-18.md')
    expect(backend.exports[0].content).toContain('# Activity Summary')
    expect(backend.exports[0].content).toContain('Improve dashboard')
  })
})
