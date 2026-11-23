import { type EnvironmentProviders, inject, provideAppInitializer, Service } from '@angular/core'
import type { LegacyImportReportDto } from '../generated/activity'
import type { ConnectionKey } from '../providers/connection'
import { ActivityBackendClient } from './backend/backend-client'
import { RuntimeEnvironmentService } from './backend/runtime-environment.service'
import { failure, failureWhile, type Notice } from './failure'
import { NoticeService } from './notice.service'
import { ToastService } from './toast.service'

/**
 * What the desktop app's one import of Activity Log 0.0.7's settings did, as a notice when the
 * user has something to do: values were left out (news, with Open Settings), the import waits
 * for the keychain or failed (with Import, which lets the keychain ask), or the file could not
 * be read. A clean import says nothing here; the Logs do.
 */
@Service()
export class LegacyImportNoticeService {
  private readonly client = inject(ActivityBackendClient)
  private readonly environment = inject(RuntimeEnvironmentService)
  private readonly notices = inject(NoticeService)
  private readonly toasts = inject(ToastService)

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

  /** The import the user asked for from a notice, the keychain allowed to ask; never rejects. */
  async importNow(): Promise<void> {
    try {
      const report = await this.client.importLegacySettings()
      const notice = legacyImportNotice(report)
      if (notice) this.notices.show(notice)
      else if (report?.kind === 'imported') this.toasts.success(report.message)
    } catch (error) {
      this.notices.show({
        origin: 'import',
        failure: failureWhile('Importing 0.0.7 settings', error),
      })
    }
  }
}

/** The notice `report` calls for, if any. */
export function legacyImportNotice(report: LegacyImportReportDto | null): Notice | null {
  if (report === null) return null
  switch (report.kind) {
    case 'imported': {
      if (report.skipped.length === 0) return null
      const first = report.skipped.find(skip => skip.connection !== undefined)?.connection
      const selection: ConnectionKey | 'logs' = first ? `${first.source}/${first.name}` : 'logs'
      return {
        origin: 'import',
        info: report.message,
        action: { kind: 'openSettings', selection },
      }
    }
    case 'postponed':
      return {
        origin: 'import',
        failure: failure(report.message),
        action:
          report.store === 'refused'
            ? { kind: 'importLegacy' }
            : { kind: 'openSettings', selection: 'home' },
      }
    case 'failed':
      return {
        origin: 'import',
        failure: failure(report.message),
        action: { kind: 'importLegacy' },
      }
    case 'unreadable':
      return { origin: 'import', failure: failure(report.message) }
  }
}

/** Asks for the import's report when the app starts. */
export function provideLegacyImportNotice(): EnvironmentProviders {
  return provideAppInitializer(() => {
    void inject(LegacyImportNoticeService).start()
  })
}
