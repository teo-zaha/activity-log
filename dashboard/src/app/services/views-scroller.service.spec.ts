import { Location, ViewportScroller } from '@angular/common'
import { provideLocationMocks } from '@angular/common/testing'
import {
  ApplicationRef,
  Component,
  type ElementRef,
  afterNextRender,
  viewChild,
} from '@angular/core'
import { TestBed } from '@angular/core/testing'
import {
  Router,
  RouterOutlet,
  provideRouter,
  withInMemoryScrolling,
  type Routes,
} from '@angular/router'
import { describe, expect, it, onTestFinished } from 'vitest'
import { restoresScroll, ViewsScroller } from './views-scroller.service'

/** Where a view starts, as a calendar view starts at its default hour. */
const START = 400

/** A scroller whose position sticks, as jsdom's does not. */
function scrollable(element: HTMLElement): void {
  let top = 0
  Object.defineProperty(element, 'scrollTop', {
    get: () => top,
    set: (value: number) => (top = value),
  })
}

@Component({ template: `<div #scroller class="scroller"></div>` })
class ScrolledView {
  readonly scroller = viewChild.required<ElementRef<HTMLElement>>('scroller')

  constructor() {
    // Before `restoresScroll` draws it, so the position it is shown at sticks.
    afterNextRender({ earlyRead: () => scrollable(this.scroller().nativeElement) })
    restoresScroll(this.scroller, () => START)
  }
}

@Component({ selector: 'app-outlet', imports: [RouterOutlet], template: `<router-outlet />` })
class Outlet {}

const ROUTES: Routes = [
  { path: '', pathMatch: 'full', redirectTo: 'week/1' },
  { path: 'week/:day', component: ScrolledView },
  { path: 'day/:day', component: ScrolledView },
]

/**
 * Boots `Outlet` as an app is booted: the router's scroll restoration starts only then, from its
 * bootstrap listener.
 */
async function openRouter(): Promise<{ router: Router; settle: () => Promise<void> }> {
  TestBed.configureTestingModule({
    providers: [
      provideLocationMocks(),
      provideRouter(ROUTES, withInMemoryScrolling({ scrollPositionRestoration: 'enabled' })),
      { provide: ViewportScroller, useExisting: ViewsScroller },
    ],
  })
  const host = document.createElement('app-outlet')
  document.body.append(host)
  onTestFinished(() => host.remove())
  const appRef = TestBed.inject(ApplicationRef)
  appRef.bootstrap(Outlet, host)
  const router = TestBed.inject(Router)
  const settle = async (): Promise<void> => {
    await appRef.whenStable()
    // The router asks for the position a task after the navigation ends.
    await new Promise(resolve => setTimeout(resolve, 20))
    await appRef.whenStable()
  }
  await settle()
  return { router, settle }
}

function scrolled(): HTMLElement {
  const element = document.querySelector<HTMLElement>('.scroller')
  if (!element) throw new Error('a view is on screen')
  return element
}

describe('ViewsScroller', () => {
  it('shows a new view where it starts', async () => {
    const { router, settle } = await openRouter()

    await router.navigateByUrl('/week/1')
    await settle()

    expect(scrolled().scrollTop).toBe(START)
  })

  it('shows a view where its history entry left it on Back and Forward', async () => {
    const { router, settle } = await openRouter()
    const location = TestBed.inject(Location)
    await router.navigateByUrl('/week/1')
    await settle()
    scrolled().scrollTop = 150
    await router.navigateByUrl('/day/1')
    await settle()
    expect(scrolled().scrollTop).toBe(START)
    scrolled().scrollTop = 900

    location.back()
    await settle()
    expect(scrolled().scrollTop).toBe(150)

    location.forward()
    await settle()
    expect(scrolled().scrollTop).toBe(900)
  })

  it('measures a position from where the view starts, so the top is one too', async () => {
    const { router, settle } = await openRouter()
    const location = TestBed.inject(Location)
    await router.navigateByUrl('/week/1')
    await settle()
    scrolled().scrollTop = 0
    await router.navigateByUrl('/day/1')
    await settle()

    location.back()
    await settle()

    expect(scrolled().scrollTop).toBe(0)
  })

  it('keeps the view where it is for a navigation that says so', async () => {
    const { router, settle } = await openRouter()
    await router.navigateByUrl('/week/1')
    await settle()
    scrolled().scrollTop = 150

    await router.navigateByUrl('/week/2', { scroll: 'manual' })
    await settle()

    expect(scrolled().scrollTop).toBe(150)
  })
})
