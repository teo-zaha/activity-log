import { Component } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest'
import { injectMinuteClock } from './clock'

@Component({ template: '' })
class ClockHost {
  readonly now = injectMinuteClock()
}

describe('injectMinuteClock', () => {
  beforeEach(() => {
    vi.useFakeTimers()
    vi.setSystemTime(new Date(2026, 10, 17, 9, 41, 30))
  })
  afterEach(() => vi.useRealTimers())

  it('ticks at the start of each minute', () => {
    const fixture = TestBed.createComponent(ClockHost)
    const start = fixture.componentInstance.now()

    vi.advanceTimersByTime(29_000)
    expect(fixture.componentInstance.now()).toBe(start)

    vi.advanceTimersByTime(1_000)
    expect(new Date(fixture.componentInstance.now()).getMinutes()).toBe(42)

    vi.advanceTimersByTime(60_000)
    expect(new Date(fixture.componentInstance.now()).getMinutes()).toBe(43)
  })

  it('stops its timer when the component is destroyed', () => {
    const first = TestBed.createComponent(ClockHost)
    const second = TestBed.createComponent(ClockHost)
    const running = vi.getTimerCount()

    first.destroy()
    expect(vi.getTimerCount()).toBe(running - 1)
    second.destroy()
    expect(vi.getTimerCount()).toBe(running - 2)
  })
})
