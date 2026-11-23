import { TestBed } from '@angular/core/testing'
import { describe, expect, it, vi } from 'vitest'
import type { LegacyFileDto, LegacyImportReportDto } from '../generated/activity'
import { ActivityBackendClient } from './backend/backend-client'
import { BackendFailure } from './backend/backend-failure'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'
import { LegacyImportNoticeService, legacyImportNotice } from './legacy-import-notice.service'
import { NoticeService } from './notice.service'

const file: LegacyFileDto = { source: 'settings', path: '/x/settings.json' }

type Imported = Extract<LegacyImportReportDto, { kind: 'imported' }>

function imported(skipped: Imported['skipped']): Imported {
  return {
    kind: 'imported',
    file,
    imported: [{ source: 'gitlab', name: 'gitlab' }],
    skipped,
    message: 'Imported 1 sources',
  } satisfies LegacyImportReportDto
}

describe('legacyImportNotice', () => {
  it('says nothing for no report or a clean import', () => {
    expect(legacyImportNotice(null)).toBeNull()
    expect(legacyImportNotice(imported([]))).toBeNull()
  })

  it('opens the source a value was left out of', () => {
    const notice = legacyImportNotice(
      imported([
        { field: 'WSL_GIT_DIRS', reason: 'invalidWslRepo' },
        {
          connection: { source: 'gitlab', name: 'gitlab' },
          field: 'GITLAB_TOKEN',
          reason: 'noAddress',
        },
      ])
    )
    expect(notice?.origin).toBe('import')
    expect(notice?.action).toEqual({ kind: 'openSettings', selection: 'gitlab/gitlab' })
  })

  it('opens the logs when no left-out value belongs to a source', () => {
    const notice = legacyImportNotice(
      imported([{ field: 'WSL_GIT_DIRS', reason: 'invalidWslRepo' }])
    )
    expect(notice?.action).toEqual({ kind: 'openSettings', selection: 'logs' })
  })

  it('offers the settings for an import that waits or could not read its file', () => {
    for (const report of [
      { kind: 'postponed', file, store: 'unavailable', message: 'waits' },
      { kind: 'unreadable', file, message: 'could not be read' },
    ] satisfies LegacyImportReportDto[]) {
      expect(legacyImportNotice(report)?.action).toEqual({
        kind: 'openSettings',
        selection: 'home',
      })
    }
  })
})

describe('LegacyImportNoticeService', () => {
  function setup(
    isTauri: boolean,
    take: () => Promise<LegacyImportReportDto | null>
  ): { takeLegacyImportReport: ReturnType<typeof vi.fn>; notices: NoticeService } {
    const takeLegacyImportReport = vi.fn(take)
    TestBed.configureTestingModule({
      providers: [
        { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => isTauri } },
        { provide: ActivityBackendClient, useValue: { takeLegacyImportReport } },
      ],
    })
    return { takeLegacyImportReport, notices: TestBed.inject(NoticeService) }
  }

  it('shows the desktop report as a notice', async () => {
    const { notices } = setup(true, () =>
      Promise.resolve({ kind: 'unreadable', file, message: 'could not be read' })
    )
    await TestBed.inject(LegacyImportNoticeService).start()
    expect(notices.current()?.origin).toBe('import')
  })

  it('asks nothing in the browser', async () => {
    const { takeLegacyImportReport, notices } = setup(false, () => Promise.resolve(null))
    await TestBed.inject(LegacyImportNoticeService).start()
    expect(takeLegacyImportReport).not.toHaveBeenCalled()
    expect(notices.current()).toBeNull()
  })

  it('logs a report that does not come and shows nothing', async () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {})
    const { notices } = setup(true, () => Promise.reject(new BackendFailure('internal', 'gone')))
    await TestBed.inject(LegacyImportNoticeService).start()
    expect(notices.current()).toBeNull()
    expect(warn).toHaveBeenCalled()
    warn.mockRestore()
  })
})
