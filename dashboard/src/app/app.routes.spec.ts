import { provideLocationMocks } from '@angular/common/testing'
import { Component, type Type } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { By, Title } from '@angular/platform-browser'
import { provideRouter, RouteReuseStrategy, RouterOutlet, Router } from '@angular/router'
import { RouterTestingHarness } from '@angular/router/testing'
import { describe, expect, it } from 'vitest'
import { provideAppRouter, TimelineReuseStrategy } from './app.routes'

@Component({ template: '<router-outlet />', imports: [RouterOutlet] })
class LayoutStub {}

@Component({ template: 'timeline' })
class TimelineStub {}

@Component({ template: 'day' })
class DayStub {}

@Component({ template: 'sibling' })
class SiblingStub {}

describe('TimelineReuseStrategy', () => {
  /** The app's shape: shell > views layout > views, and a page beside the views layout. */
  async function harness(): Promise<RouterTestingHarness> {
    TestBed.configureTestingModule({
      providers: [
        provideRouter([
          {
            path: '',
            component: LayoutStub,
            children: [
              {
                path: '',
                component: LayoutStub,
                children: [
                  { path: 'timeline', component: TimelineStub },
                  { path: 'day/:date', component: DayStub },
                ],
              },
              { path: 'sibling', component: SiblingStub },
            ],
          },
        ]),
        { provide: RouteReuseStrategy, useClass: TimelineReuseStrategy },
      ],
    })
    return RouterTestingHarness.create()
  }

  /** The instance of `view` the page shows after navigating to `url`. */
  async function shown<T>(app: RouterTestingHarness, url: string, view: Type<T>): Promise<T> {
    await app.navigateByUrl(url)
    const found: unknown = app.fixture.debugElement.query(By.directive(view))?.componentInstance
    if (!(found instanceof view)) throw new Error(`${url} shows no ${view.name}`)
    return found
  }

  it('puts the same timeline back after a calendar view, and rebuilds calendar views', async () => {
    const app = await harness()

    const timeline = await shown(app, '/timeline', TimelineStub)
    const day = await shown(app, '/day/2026-11-17', DayStub)
    expect(await shown(app, '/timeline', TimelineStub)).toBe(timeline)
    expect(await shown(app, '/day/2026-11-17', DayStub)).not.toBe(day)
  })

  it('keeps the timeline across a page beside the views’ layout, which is built again', async () => {
    const app = await harness()

    const timeline = await shown(app, '/timeline', TimelineStub)
    const layouts = app.fixture.debugElement.queryAll(By.directive(LayoutStub))
    await shown(app, '/sibling', SiblingStub)
    expect(await shown(app, '/timeline', TimelineStub)).toBe(timeline)
    const views = app.fixture.debugElement.queryAll(By.directive(LayoutStub))[1]
    expect(views?.componentInstance).not.toBe(layouts[1]?.componentInstance)
  })
})

describe('APP_ROUTES', () => {
  async function navigate(url: string): Promise<Router> {
    TestBed.configureTestingModule({ providers: [provideAppRouter(), provideLocationMocks()] })
    const router = TestBed.inject(Router)
    await router.navigateByUrl(url)
    return router
  }

  it('titles the page after the deepest route, the view and its day', async () => {
    await navigate('/day/2026-11-17')

    expect(TestBed.inject(Title).getTitle()).toBe('Day · Tuesday, 17 November 2026 · Activity Log')
  })

  it('shows today for a calendar view without a real day, keeping the query', async () => {
    const router = await navigate('/week/someday?settings=logs')

    expect(router.url).toMatch(/^\/week\/\d{4}-\d{2}-\d{2}\?settings=logs$/)
  })
})
