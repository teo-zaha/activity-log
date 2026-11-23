import { type EnvironmentProviders, inject, provideAppInitializer, Service } from '@angular/core'
import type { LegacyImportReportDto } from '../generated/activity'
import type { ConnectionKey } from '../providers/connection'
import { ActivityBackendClient } from './backend/backend-client'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'
import { failure, failureWhile, type Notice } from './failure'
import { NoticeService } from './notice.service'

/**
 * What the desktop app's one import of Activity Log 0.0.7's settings did, as a notice with Open
 * Settings when the user has something to do: values were left out, the import waits for the
 * secure store, or the file could not be read. A clean import says nothing here; the Logs do.
 */
@Service()
export class LegacyImportNoticeService {
  private readonly client = inject(ActivityBackendClient)
  private readonly environment = inject(RuntimeEnvironmentService)
  private readonly notices = inject(NoticeService)

  /** Never rejects: a report that does not come is logged. The browser has none. */
  async start(): Promise<void> {
    if (!this.environment.isTauri()) return
    try {
      const notice = legacyImportNotice(await this.client.takeLegacyImportReport())
      if (notice) this.notices.show(notice)
    } catch (error) {
      const failed = failureWhile('Reading what the import of 0.0.7 settings did', error)
      console.warn(failed.message, failed.cause)
    }
  }
}

/** The notice `report` calls for, if any; Open Settings goes where there is something to do. */
export function legacyImportNotice(report: LegacyImportReportDto | null): Notice | null {
  if (report === null) return null
  switch (report.kind) {
    case 'imported': {
      const first = report.skipped.find(skip => skip.connection !== undefined)?.connection
      if (report.skipped.length === 0) return null
      const selection: ConnectionKey | 'logs' = first ? `${first.source}/${first.name}` : 'logs'
      return {
        origin: 'import',
        failure: failure(report.message),
        action: { kind: 'openSettings', selection },
      }
    }
    case 'postponed':
    case 'unreadable':
      return {
        origin: 'import',
        failure: failure(report.message),
        action: { kind: 'openSettings', selection: 'home' },
      }
  }
}

/** Asks for the import's report when the app starts. */
export function provideLegacyImportNotice(): EnvironmentProviders {
  return provideAppInitializer(() => {
    void inject(LegacyImportNoticeService).start()
  })
}
