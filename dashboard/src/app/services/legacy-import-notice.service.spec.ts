import { TestBed } from '@angular/core/testing'
import { describe, expect, it, vi } from 'vitest'
import type { LegacyFileDto, LegacyImportReportDto } from '../generated/activity'
import { ActivityBackendClient } from './backend/backend-client'
import { BackendFailure } from './backend/backend-failure'
import type { NoticeAction } from './failure'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'
import { LegacyImportNoticeService, legacyImportNotice } from './legacy-import-notice.service'
import { NoticeService } from './notice.service'
import { ToastService } from './toast.service'

const file: LegacyFileDto = { source: 'settings', path: '/x/settings.json' }

type Imported = Extract<LegacyImportReportDto, { kind: 'imported' }>

function imported(skipped: Imported['skipped']): Imported {
  return {
    kind: 'imported',
    file,
    imported: [{ source: 'gitlab', name: 'gitlab' }],
    skipped,
    message: 'Imported 1 sources',
  }
}

describe('legacyImportNotice', () => {
  it('says nothing for no report or a clean import', () => {
    expect(legacyImportNotice(null)).toBeNull()
    expect(legacyImportNotice(imported([]))).toBeNull()
  })

  it('shows left-out values as news, not a failure, opening the source they were for', () => {
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
    expect(notice && 'failure' in notice).toBe(false)
    expect(notice?.action).toEqual({ kind: 'openSettings', selection: 'gitlab/gitlab' })
  })

  it('opens the logs when no left-out value belongs to a source', () => {
    const notice = legacyImportNotice(
      imported([{ field: 'WSL_GIT_DIRS', reason: 'invalidWslRepo' }])
    )
    expect(notice?.action).toEqual({ kind: 'openSettings', selection: 'logs' })
  })

  it('offers the import with the keychain asking where it was refused or failed', () => {
    const waits = (report: LegacyImportReportDto): NoticeAction | undefined =>
      legacyImportNotice(report)?.action
    expect(waits({ kind: 'postponed', file, store: 'refused', message: 'waits' })).toEqual({
      kind: 'importLegacy',
    })
    expect(waits({ kind: 'failed', file, message: 'failed' })).toEqual({ kind: 'importLegacy' })
    expect(waits({ kind: 'postponed', file, store: 'unavailable', message: 'waits' })).toEqual({
      kind: 'openSettings',
      selection: 'home',
    })
    expect(waits({ kind: 'unreadable', file, message: 'could not be read' })).toBeUndefined()
  })
})

describe('LegacyImportNoticeService', () => {
  function setup(
    isTauri: boolean,
    take: () => Promise<LegacyImportReportDto | null>,
    importNow: () => Promise<LegacyImportReportDto | null> = () => Promise.resolve(null)
  ): {
    takeLegacyImportReport: ReturnType<typeof vi.fn>
    notices: NoticeService
    success: ReturnType<typeof vi.fn>
  } {
    const takeLegacyImportReport = vi.fn(take)
    const success = vi.fn()
    TestBed.configureTestingModule({
      providers: [
        { provide: RuntimeEnvironmentService, useValue: { isTauri: (): boolean => isTauri } },
        {
          provide: ActivityBackendClient,
          useValue: { takeLegacyImportReport, importLegacySettings: vi.fn(importNow) },
        },
        { provide: ToastService, useValue: { success } },
      ],
    })
    return { takeLegacyImportReport, notices: TestBed.inject(NoticeService), success }
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

  it('confirms an import the user asked for, and shows one refused again', async () => {
    const answers: (LegacyImportReportDto | null)[] = [
      imported([]),
      { kind: 'postponed', file, store: 'refused', message: 'waits' },
    ]
    const { notices, success } = setup(
      true,
      () => Promise.resolve(null),
      () => Promise.resolve(answers.shift() ?? null)
    )
    const service = TestBed.inject(LegacyImportNoticeService)
    await service.importNow()
    expect(success).toHaveBeenCalledWith('Imported 1 sources')
    expect(notices.current()).toBeNull()
    await service.importNow()
    expect(notices.current()?.action).toEqual({ kind: 'importLegacy' })
  })
})
