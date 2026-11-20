import { TestBed } from '@angular/core/testing'
import { emit } from '@tauri-apps/api/event'
import { mockIPC, mockWindows } from '@tauri-apps/api/mocks'
import { clearTauriMocks } from '../../testing/tauri-mocks'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { RuntimeEnvironmentService } from './runtime-environment.service'

describe('RuntimeEnvironmentService', () => {
  afterEach(() => {
    clearTauriMocks()
    document.documentElement.classList.remove('fullscreen')
  })

  it('follows a fullscreen restore that lands right after the first read', async () => {
    let fullscreen = false
    let reads = 0
    mockWindows('main')
    mockIPC(
      cmd => {
        if (cmd !== 'plugin:window|is_fullscreen') return undefined
        reads++
        const answer = fullscreen
        if (reads === 1) {
          // The desktop app enters fullscreen just after the window shows.
          fullscreen = true
          void emit('tauri://resize', { width: 1920, height: 1080 })
        }
        return answer
      },
      { shouldMockEvents: true }
    )

    TestBed.inject(RuntimeEnvironmentService)

    // The window API loads on demand and each read is a round trip: wait for what they show.
    await vi.waitFor(() =>
      expect(document.documentElement.classList.contains('fullscreen')).toBe(true)
    )
    expect(reads).toBeGreaterThan(1)
  })
})
