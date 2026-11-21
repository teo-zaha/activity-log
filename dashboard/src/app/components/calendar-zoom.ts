import {
  Injector,
  afterNextRender,
  computed,
  inject,
  type ElementRef,
  type Signal,
} from '@angular/core'
import { listener } from '@signality/core/browser/listener'
import { ActivityViewStateService } from '../services/activity-view-state.service'
import {
  DEFAULT_HOUR_HEIGHT,
  MAX_HOUR_HEIGHT,
  MIN_HOUR_HEIGHT,
  anchoredScrollTop,
  clampHourHeight,
  stepHourHeight,
  zoomPercent,
} from '../utils/calendar-zoom'

/** How fast Ctrl/⌘ + wheel zooms, per pixel of wheel delta. */
const WHEEL_RATE = 0.01

/** WebKit's pinch event (Safari, WKWebView), which has no type in the DOM library. */
interface GestureEvent extends UIEvent {
  readonly scale: number
  readonly clientY: number
}

function isGestureEvent(event: Event): event is GestureEvent {
  return 'scale' in event && typeof event.scale === 'number' && 'clientY' in event
}

export interface CalendarZoom {
  readonly percent: Signal<string>
  readonly atDefault: Signal<boolean>
  readonly canZoomIn: Signal<boolean>
  readonly canZoomOut: Signal<boolean>
  zoomIn(): void
  zoomOut(): void
  reset(): void
}

/**
 * Zoom for the Day and Week grids: the buttons step through `ZOOM_LEVELS`, Ctrl/⌘ + wheel and
 * pinch zoom smoothly, and every change keeps the time under the pointer (or in the middle of
 * the view, for the buttons) where it was. `grid` is the hour grid inside `scroller`.
 */
export function injectCalendarZoom(
  scroller: Signal<ElementRef<HTMLElement>>,
  grid: Signal<ElementRef<HTMLElement>>
): CalendarZoom {
  const hourHeight = inject(ActivityViewStateService).hourHeight
  const injector = inject(Injector)
  /** The scroll position waiting for the next render; wheel events can come faster. */
  let pendingScrollTop: number | null = null

  const zoomTo = (next: number, anchorY?: number): void => {
    const from = hourHeight()
    const to = clampHourHeight(next)
    if (Math.abs(to - from) < 0.01) return
    const element = scroller().nativeElement
    const scrollTop = pendingScrollTop ?? element.scrollTop
    const gridTop =
      grid().nativeElement.getBoundingClientRect().top -
      element.getBoundingClientRect().top +
      element.scrollTop
    // The all-day row above the grid is sticky: the middle of the view is the middle of what it
    // leaves visible.
    const view = element.clientHeight
    const anchor = anchorY ?? (Math.min(gridTop, view) + view) / 2
    const scheduled = pendingScrollTop !== null
    pendingScrollTop = anchoredScrollTop(scrollTop, gridTop, anchor, from, to)
    hourHeight.set(to)
    if (scheduled) return
    afterNextRender(
      {
        write: () => {
          if (pendingScrollTop !== null) element.scrollTop = pendingScrollTop
          pendingScrollTop = null
        },
      },
      { injector }
    )
  }

  const offsetY = (event: { clientY: number }): number =>
    event.clientY - scroller().nativeElement.getBoundingClientRect().top

  // Not passive: on an element (not the window or document) a wheel listener may cancel the
  // browser's own page zoom.
  listener(scroller, 'wheel', (event: WheelEvent) => {
    if (!event.ctrlKey && !event.metaKey) return
    event.preventDefault()
    const delta = event.deltaMode === WheelEvent.DOM_DELTA_LINE ? event.deltaY * 16 : event.deltaY
    zoomTo(hourHeight() * Math.exp(-delta * WHEEL_RATE), offsetY(event))
  })
  let gestureBase = hourHeight()
  listener(scroller, 'gesturestart', (event: Event) => {
    event.preventDefault()
    gestureBase = hourHeight()
  })
  listener(scroller, 'gesturechange', (event: Event) => {
    if (!isGestureEvent(event)) return
    event.preventDefault()
    zoomTo(gestureBase * event.scale, offsetY(event))
  })

  return {
    percent: computed(() => zoomPercent(hourHeight())),
    atDefault: computed(() => Math.abs(hourHeight() - DEFAULT_HOUR_HEIGHT) < 0.5),
    canZoomIn: computed(() => hourHeight() < MAX_HOUR_HEIGHT - 0.5),
    canZoomOut: computed(() => hourHeight() > MIN_HOUR_HEIGHT + 0.5),
    zoomIn: () => zoomTo(stepHourHeight(hourHeight(), 1)),
    zoomOut: () => zoomTo(stepHourHeight(hourHeight(), -1)),
    reset: () => zoomTo(DEFAULT_HOUR_HEIGHT),
  }
}
