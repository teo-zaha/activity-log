import {
  Component,
  DestroyRef,
  ElementRef,
  afterNextRender,
  computed,
  inject,
  input,
  output,
  signal,
  viewChild,
} from '@angular/core'
import type { DayIndexDay, DayIndexItem } from '../../../models/day-index'
import { densityFor, nearestDayWithEntries, slotAt } from '../../../utils/day-index'
import { longDayLabel, shortDayLabel } from '../../../utils/local-day'

// Keep in step with the stylesheet: the track's inset and the month heading heights.
const TRACK_INSET = 6
const MONTH_HEIGHT = 14
const FIRST_MONTH_HEIGHT = 12
const MAX_MAGNIFICATION_REACH = 3.2

interface RailLayout {
  /** Vertical centre of each day row, relative to the rail's top edge. */
  readonly centres: readonly number[]
  readonly slot: number
}

interface Bubble {
  readonly y: number
  readonly label: string
  readonly count: string
}

function eventCount(count: number): string {
  return `${count} ${count === 1 ? 'event' : 'events'}`
}

/**
 * The side day index of the timeline: one row per day of the range, newest at the top.
 * Hovering previews a day, pressing or dragging jumps to it, and the keyboard steps through
 * the days that have events. Empty days snap to the nearest day with events.
 */
@Component({
  selector: 'app-day-index',
  templateUrl: './day-index.component.html',
  styleUrl: './day-index.component.scss',
  host: {
    role: 'slider',
    tabindex: '0',
    'aria-label': 'Jump to day',
    'aria-orientation': 'vertical',
    'aria-valuemin': '0',
    '[attr.aria-valuemax]': 'targets().length - 1',
    '[attr.aria-valuenow]': 'valueNow()',
    '[attr.aria-valuetext]': 'valueText()',
    '[attr.data-density]': 'density()',
    '[class.is-hovering]': 'hovering()',
    '[class.is-scrubbing]': 'scrubbing()',
    '(pointerenter)': 'onPointerEnter($event)',
    '(pointermove)': 'onPointerMove($event)',
    '(pointerleave)': 'onPointerLeave()',
    '(pointerdown)': 'onPointerDown($event)',
    '(pointerup)': 'endScrub($event)',
    '(pointercancel)': 'endScrub($event)',
    '(keydown)': 'onKeydown($event)',
  },
})
export class DayIndexComponent {
  readonly items = input.required<readonly DayIndexItem[]>()
  /** The day whose header is stuck at the top of the list. */
  readonly activeDay = input.required<string | null>()
  /** Scroll the list to this `YYYY-MM-DD` day. */
  readonly jump = output<string>()
  /** Move keyboard focus into this day's events. */
  readonly focusDay = output<string>()

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
  private readonly track = viewChild.required<ElementRef<HTMLElement>>('track')
  private readonly trackHeight = signal(0)
  private readonly reducedMotion =
    typeof matchMedia === 'function' && matchMedia('(prefers-reduced-motion: reduce)').matches

  protected readonly hovering = signal(false)
  protected readonly scrubbing = signal(false)
  /** Pointer position relative to the rail's top edge, while the pointer previews a day. */
  private readonly pointerY = signal<number | null>(null)
  private pendingY = 0
  private frame = 0
  private lastJump: string | null = null

  protected readonly days = computed(() =>
    this.items().filter((item): item is DayIndexDay => item.kind === 'day')
  )

  /** Indexes into `days()` of the days that have events. */
  protected readonly targets = computed(() =>
    this.days().flatMap((day, index) => (day.count > 0 ? [index] : []))
  )

  private readonly multiYear = computed(() => {
    const days = this.days()
    return days.length > 0 && days[0].key.slice(0, 4) !== days[days.length - 1].key.slice(0, 4)
  })

  /** Day rows share what the month headings leave of the track, as the flex layout does. */
  private readonly layout = computed<RailLayout>(() => {
    const items = this.items()
    const months = items.length - this.days().length
    const headings = months > 0 ? FIRST_MONTH_HEIGHT + (months - 1) * MONTH_HEIGHT : 0
    const slot = Math.max(0, (this.trackHeight() - headings) / Math.max(1, this.days().length))
    const centres: number[] = []
    let y = TRACK_INSET
    items.forEach((item, index) => {
      if (item.kind === 'month') {
        y += index === 0 ? FIRST_MONTH_HEIGHT : MONTH_HEIGHT
      } else {
        centres.push(y + slot / 2)
        y += slot
      }
    })
    return { centres, slot }
  })

  protected readonly density = computed(() => densityFor(this.layout().slot))

  private readonly activeIndex = computed(() => {
    const key = this.activeDay()
    return this.days().findIndex(day => day.key === key)
  })

  /** The day a press at the pointer would land on. */
  private readonly previewIndex = computed(() => {
    const y = this.pointerY()
    if (y === null) return -1
    return nearestDayWithEntries(this.days(), slotAt(this.layout().centres, y))
  })

  protected readonly pill = computed(() => {
    const day = this.days()[this.activeIndex()]
    return day ? { y: this.layout().centres[this.activeIndex()], label: day.dayOfMonth } : null
  })

  protected readonly bubble = computed<Bubble | null>(() => {
    const preview = this.previewIndex()
    const index = preview >= 0 ? preview : this.activeIndex()
    const day = this.days()[index]
    if (!day) return null
    return {
      y: this.pointerY() ?? this.layout().centres[index],
      label: shortDayLabel(day.key, this.multiYear()),
      count: eventCount(day.count),
    }
  })

  /** Dock-like magnification of the rows near the pointer, 0 to 1, by day key. */
  protected readonly magnification = computed<ReadonlyMap<string, number>>(() => {
    const y = this.pointerY()
    const { centres, slot } = this.layout()
    if (y === null || this.density() === 'dense' || this.reducedMotion) return new Map()
    const reach = Math.max(slot * MAX_MAGNIFICATION_REACH, 28)
    const near = new Map<string, number>()
    this.days().forEach((day, index) => {
      const distance = Math.abs(centres[index] - y)
      if (distance <= reach) near.set(day.key, Math.cos(((distance / reach) * Math.PI) / 2) ** 2)
    })
    return near
  })

  /** The newest day is the slider's maximum, as it sits at the top. */
  protected readonly valueNow = computed(() => {
    const at = this.targets().indexOf(this.activeIndex())
    return at < 0 ? null : this.targets().length - 1 - at
  })

  protected readonly valueText = computed(() => {
    const day = this.days()[this.activeIndex()]
    return day ? `${longDayLabel(day.key)}, ${eventCount(day.count)}` : null
  })

  constructor() {
    const destroyRef = inject(DestroyRef)
    afterNextRender(() => {
      const track = this.track().nativeElement
      this.trackHeight.set(track.clientHeight)
      const observer = new ResizeObserver(() => this.trackHeight.set(track.clientHeight))
      observer.observe(track)
      destroyRef.onDestroy(() => observer.disconnect())
    })
    destroyRef.onDestroy(() => cancelAnimationFrame(this.frame))
  }

  protected onPointerEnter(event: PointerEvent): void {
    if (event.pointerType === 'mouse') this.hovering.set(true)
  }

  protected onPointerMove(event: PointerEvent): void {
    if (event.pointerType === 'mouse' || this.scrubbing()) this.queue(event)
  }

  protected onPointerLeave(): void {
    if (this.scrubbing()) return
    this.hovering.set(false)
    this.pointerY.set(null)
  }

  protected onPointerDown(event: PointerEvent): void {
    if (event.button !== 0) return
    event.preventDefault()
    this.host.setPointerCapture(event.pointerId)
    this.scrubbing.set(true)
    this.lastJump = null
    // At once, not on the next frame: a quick click may release before the frame comes.
    this.pendingY = this.offsetOf(event)
    this.applyPointer()
  }

  protected endScrub(event: PointerEvent): void {
    if (!this.scrubbing()) return
    this.scrubbing.set(false)
    if (event.pointerType !== 'mouse' || !this.host.matches(':hover')) {
      this.hovering.set(false)
      this.pointerY.set(null)
    }
  }

  protected onKeydown(event: KeyboardEvent): void {
    const days = this.days()
    const targets = this.targets()
    const active = this.activeIndex()
    if (event.key === 'Enter') {
      event.preventDefault()
      if (active >= 0) this.focusDay.emit(days[active].key)
      return
    }
    const at = targets.indexOf(active)
    const byDays = (offset: number): number => {
      const index = Math.min(days.length - 1, Math.max(0, active + offset))
      return targets.indexOf(nearestDayWithEntries(days, index))
    }
    const steps: Partial<Record<string, number>> = {
      ArrowDown: at + 1,
      ArrowUp: at - 1,
      PageDown: byDays(7),
      PageUp: byDays(-7),
      Home: 0,
      End: targets.length - 1,
    }
    const next = at < 0 && event.key.startsWith('Arrow') ? 0 : steps[event.key]
    if (next === undefined || targets.length === 0) return
    event.preventDefault()
    const target = days[targets[Math.min(targets.length - 1, Math.max(0, next))]]
    if (target.key !== this.activeDay()) this.jump.emit(target.key)
  }

  /** Applies the latest pointer position once per frame. */
  private queue(event: PointerEvent): void {
    this.pendingY = this.offsetOf(event)
    if (this.frame) return
    this.frame = requestAnimationFrame(() => {
      this.frame = 0
      this.applyPointer()
    })
  }

  private applyPointer(): void {
    this.pointerY.set(this.pendingY)
    if (!this.scrubbing()) return
    const day = this.days()[this.previewIndex()]
    if (day && day.key !== this.activeDay() && day.key !== this.lastJump) {
      this.lastJump = day.key
      this.jump.emit(day.key)
    }
  }

  private offsetOf(event: PointerEvent): number {
    return event.clientY - this.host.getBoundingClientRect().top
  }
}
