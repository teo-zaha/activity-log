import { TestBed } from '@angular/core/testing'
import { describe, expect, it } from 'vitest'
import { failure } from './failure'
import { NoticeService } from './notice.service'

describe('NoticeService', () => {
  it('shows a notice and clears it only for a new attempt of the same action', () => {
    const notices = TestBed.inject(NoticeService)
    const exportFailed = { origin: 'export', failure: failure('Export failed') } as const

    notices.show(exportFailed)
    notices.clear('view')
    expect(notices.current()).toBe(exportFailed)

    notices.clear('export')
    expect(notices.current()).toBeNull()
  })
})
