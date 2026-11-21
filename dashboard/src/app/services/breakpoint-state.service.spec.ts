import { TestBed } from '@angular/core/testing'
import { describe, expect, it, onTestFinished } from 'vitest'
import { mockBreakpoints } from '../testing/media'
import { BreakpointState } from './breakpoint-state.service'

describe('BreakpointState', () => {
  it('follows each named width, and shows the rail only between a phone’s and a tablet’s', () => {
    const media = mockBreakpoints({ phone: false, tablet: false, 'desktop-compact': false })
    onTestFinished(() => media.restore())
    const state = TestBed.inject(BreakpointState)
    // Each media query is read, and its changes followed, from an effect.
    TestBed.tick()
    const read = (): Record<string, boolean> => ({
      phone: state.phone(),
      tablet: state.tablet(),
      desktopCompact: state.desktopCompact(),
      rail: state.rail(),
    })
    expect(read()).toEqual({ phone: false, tablet: false, desktopCompact: false, rail: false })

    media.set('desktop-compact', true)
    expect(read()).toEqual({ phone: false, tablet: false, desktopCompact: true, rail: false })

    media.set('tablet', true)
    expect(read()).toEqual({ phone: false, tablet: true, desktopCompact: true, rail: true })

    media.set('phone', true)
    expect(read()).toEqual({ phone: true, tablet: true, desktopCompact: true, rail: false })
  })
})
