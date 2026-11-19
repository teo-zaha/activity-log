import { Component } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { provideRouter, RouteReuseStrategy } from '@angular/router'
import { RouterTestingHarness } from '@angular/router/testing'
import { describe, expect, it } from 'vitest'
import { TimelineReuseStrategy } from './app.routes'

@Component({ template: 'timeline' })
class TimelineStub {}

@Component({ template: 'day' })
class DayStub {}

describe('TimelineReuseStrategy', () => {
  it('puts the same timeline back after a calendar view, and rebuilds calendar views', async () => {
    TestBed.configureTestingModule({
      providers: [
        provideRouter([
          { path: 'timeline', component: TimelineStub },
          { path: 'day/:date', component: DayStub },
        ]),
        { provide: RouteReuseStrategy, useClass: TimelineReuseStrategy },
      ],
    })
    const harness = await RouterTestingHarness.create()

    const timeline = await harness.navigateByUrl('/timeline', TimelineStub)
    const day = await harness.navigateByUrl('/day/2026-11-17', DayStub)
    expect(await harness.navigateByUrl('/timeline', TimelineStub)).toBe(timeline)
    expect(await harness.navigateByUrl('/day/2026-11-17', DayStub)).not.toBe(day)
  })
})
