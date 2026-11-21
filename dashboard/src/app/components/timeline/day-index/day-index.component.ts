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
import { resizeObserver } from '@signality/core/observers/resize-observer'
import type { DayIndexDay, DayIndexDensity, DayIndexItem } from '../../../models/day-index'
import {
  densityFor,
  isMonthDensity,
  monthDensityFor,
  monthRowLabel,
  monthRows,
  nearestDayWithEntries,
  slotAt,
} from '../../../utils/day-index'
import { longDayLabel, type DayKey } from '../../../utils/local-day'

// Keep in step with the stylesheet: the track's inset and the month heading heights.
const TRACK_INSET = 6
const MONTH_HEIGHT = 14
const FIRST_MONTH_HEIGHT = 12
/** Dense rows are thinner than the pill; this keeps the pill off the top month label. */
const DENSE_FIRST_MONTH_HEIGHT = 24
/** A few days do not stretch over the whole rail: their rows stop growing here, at the top. */
const MAX_DAY_SLOT = 24
/** The pill's height on a day; months have a thinner one, see `pill`. */
const DAY_PILL_HEIGHT = 22
/** The pill shrinks down to this to make room for the magnified day beside it. */
const DAY_PILL_MIN_HEIGHT = 14
/** The pill's height on a month: thinner month rows get a bare marker instead. */
const MONTH_PILL_HEIGHT = 18
/** The bump behind the hovered day (`.bulge`). */
const BULGE_HEIGHT = 30
/** Room kept between the bump and the pill beside it. */
const BULGE_GAP = 2
/** The least the bump reaches up and down: enough to hold the magnified number. */
const MIN_BULGE_HALF = 12

interface RailLayout {
  readonly density: DayIndexDensity
  /** Vertical centre of each day, relative to the rail's top edge. */
  readonly centres: readonly number[]
  readonly slot: number
  /** The height of one month row; 0 unless the rail shows months. */
  readonly monthHeight: number
}

interface Bulge {
  readonly y: number
  /** How far it reaches up and down from `y`. */
  readonly half: number
}

interface Pill {
  readonly y: number
  /** Null for the bare marker beside a month row too thin for the pill. */
  readonly label: string | null
  readonly height: number
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
    '[class.has-bare-pill]': 'pill()?.label === null',
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
  readonly activeDay = input.required<DayKey | null>()
  /** Scroll the list to this day. */
  readonly jump = output<DayKey>()
  /** Move keyboard focus into this day's events. */
  readonly focusDay = output<DayKey>()

  private readonly host = inject<ElementRef<HTMLElement>>(ElementRef).nativeElement
  private readonly track = viewChild.required<ElementRef<HTMLElement>>('track')
  private readonly trackHeight = signal(0)

  protected readonly hovering = signal(false)
  protected readonly scrubbing = signal(false)
  /** Pointer position relative to the rail's top edge, while the pointer previews a day. */
  private readonly pointerY = signal<number | null>(null)
  private pendingY = 0
  private frame = 0
  private lastJump: DayKey | null = null

  protected readonly days = computed(() =>
    this.items().filter((item): item is DayIndexDay => item.kind === 'day')
  )

  /** Indexes into `days()` of the days that have events. */
  protected readonly targets = computed(() =>
    this.days().flatMap((day, index) => (day.count > 0 ? [index] : []))
  )

  /** The months of the range, for a rail too short to show days. */
  protected readonly monthRows = computed(() => monthRows(this.items()))

  private readonly layout = computed<RailLayout>(() => {
    const layout = this.layoutWith(FIRST_MONTH_HEIGHT)
    switch (layout.density) {
      case 'dense':
        return { ...this.layoutWith(DENSE_FIRST_MONTH_HEIGHT), density: 'dense' }
      case 'months':
        // Until the track is measured every range looks too long; stay on days meanwhile.
        return this.trackHeight() > 0
          ? this.monthLayout()
          : { ...this.layoutWith(DENSE_FIRST_MONTH_HEIGHT), density: 'dense' }
      default:
        return layout
    }
  })

  protected readonly density = computed(() => this.layout().density)
  /** Rows are drawn once the track is measured: until then every range would show its days. */
  protected readonly measured = computed(() => this.trackHeight() > 0)
  protected readonly showsMonths = computed(() => isMonthDensity(this.density()))

  /** The month rows with what each shows at the current density; null labels draw a tick. */
  protected readonly monthMarks = computed(() => {
    const density = this.density()
    if (!isMonthDensity(density)) return []
    return this.monthRows().map(row => ({ row, label: monthRowLabel(row, density) }))
  })

  /**
   * Months share the track evenly, so each stays easy to hit however many days it has in the
   * range; a month's days spread over its row, so dragging still lands on a day.
   */
  private monthLayout(): RailLayout {
    const rows = this.monthRows()
    const monthHeight = this.trackHeight() / Math.max(1, rows.length)
    const centres: number[] = []
    rows.forEach((row, index) => {
      const top = TRACK_INSET + index * monthHeight
      for (let day = 0; day < row.days; day++) {
        centres.push(top + ((day + 0.5) * monthHeight) / row.days)
      }
    })
    return { density: monthDensityFor(monthHeight), centres, slot: monthHeight / 31, monthHeight }
  }

  /** Day rows share what the month headings leave of the track, as the flex layout does. */
  private layoutWith(firstMonthHeight: number): RailLayout {
    const items = this.items()
    const months = items.length - this.days().length
    const headings = months > 0 ? firstMonthHeight + (months - 1) * MONTH_HEIGHT : 0
    const slot = Math.min(
      MAX_DAY_SLOT,
      Math.max(0, (this.trackHeight() - headings) / Math.max(1, this.days().length))
    )
    const centres: number[] = []
    let y = TRACK_INSET
    items.forEach((item, index) => {
      if (item.kind === 'month') {
        y += index === 0 ? firstMonthHeight : MONTH_HEIGHT
      } else {
        centres.push(y + slot / 2)
        y += slot
      }
    })
    return { density: densityFor(slot), centres, slot, monthHeight: 0 }
  }

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

  /** The day a press would land on, while the pointer is over the rail. */
  protected readonly previewKey = computed(() => this.days()[this.previewIndex()]?.key ?? null)

  /**
   * The active day, or in month modes its month. Month rows thinner than the pill get a bare
   * marker beside them instead, and keep their own label.
   */
  protected readonly pill = computed<Pill | null>(() => {
    if (!this.measured()) return null
    const day = this.days()[this.activeIndex()]
    if (!day) return null
    const { density, centres, monthHeight } = this.layout()
    if (!isMonthDensity(density)) {
      return {
        y: centres[this.activeIndex()],
        label: `${day.dayOfMonth}`,
        height: this.dayPillHeight(),
      }
    }
    const month = this.activeMonthIndex()
    const y = TRACK_INSET + (month + 0.5) * monthHeight
    if (monthHeight < MONTH_PILL_HEIGHT)
      return { y, label: null, height: Math.max(4, monthHeight - 2) }
    return { y, label: this.monthRows()[month].label, height: MONTH_PILL_HEIGHT }
  })

  private readonly activeMonthIndex = computed(() => {
    const key = this.activeDay()?.slice(0, 7)
    return this.monthRows().findIndex(row => row.key === key)
  })

  protected readonly activeMonth = computed(() => this.monthRows()[this.activeMonthIndex()]?.key)

  protected readonly previewMonth = computed(() => this.previewKey()?.slice(0, 7) ?? null)

  /**
   * The pill's height on a day. With the day beside it magnified, it gets smaller, down to
   * `DAY_PILL_MIN_HEIGHT`, so that day's bump fits on its own row.
   */
  private readonly dayPillHeight = computed(() => {
    const active = this.activeIndex()
    const preview = this.previewIndex()
    // `preview` is -1 without a pointer; next to the newest day (0) that is no neighbour.
    if (!this.magnifies() || preview < 0 || Math.abs(active - preview) !== 1) return DAY_PILL_HEIGHT
    const { centres } = this.layout()
    const gap = Math.abs(centres[active] - centres[preview])
    const fits = 2 * (gap - BULGE_GAP - MIN_BULGE_HALF)
    return Math.max(DAY_PILL_MIN_HEIGHT, Math.min(DAY_PILL_HEIGHT, fits))
  })

  /**
   * The bump that holds the magnified day, on the day a press would land on: its centre and how
   * far it reaches up and down, always centred on the day's row so the number sits in its middle.
   * Beside the pill it is shorter, and the pill gets thinner to make room (`dayPillHeight`); it
   * never goes below `MIN_BULGE_HALF`, which the pill does not mind, as the bump sticks out left
   * of the rail and the pill stays inside it. Null without a pointer over the rail, on the active
   * day (the pill grows instead) and on rails of bars or months.
   */
  protected readonly bulge = computed<Bulge | null>(() => {
    if (!this.magnifies() || this.holdsPill()) return null
    const { centres } = this.layout()
    const preview = this.previewIndex()
    const y = centres[preview]
    if (y === undefined) return null
    const active = this.activeIndex()
    if (Math.abs(active - preview) !== 1) return { y, half: BULGE_HEIGHT / 2 }
    const room = Math.abs(centres[active] - y) - this.dayPillHeight() / 2 - BULGE_GAP
    return { y, half: Math.max(MIN_BULGE_HALF, Math.min(BULGE_HEIGHT / 2, room)) }
  })

  /** Rails of day numbers magnify the hovered day; rails of bars and months do not. */
  private readonly magnifies = computed(() => {
    const density = this.density()
    return this.measured() && (density === 'full' || density === 'sparse')
  })

  /** The pointer is on the active day: the pill itself grows out of the rail, in place of the bump. */
  protected readonly holdsPill = computed(
    () => this.magnifies() && this.previewIndex() >= 0 && this.previewIndex() === this.activeIndex()
  )

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
    const measure = (): void => {
      const track = this.track().nativeElement
      // Out of the document while the router keeps the timeline: keep the last height.
      if (track.isConnected) this.trackHeight.set(track.clientHeight)
    }
    afterNextRender(measure)
    resizeObserver(this.track, measure)
    inject(DestroyRef).onDestroy(() => cancelAnimationFrame(this.frame))
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
    // A week on: the day with events nearest to it, but always past the active day, so a page
    // reaches the next day with events however far it is.
    const byDays = (offset: number): number => {
      const goal = active + offset
      let best = -1
      targets.forEach((day, index) => {
        const ahead = offset > 0 ? day > active : day < active
        if (ahead && (best < 0 || Math.abs(day - goal) < Math.abs(targets[best] - goal))) {
          best = index
        }
      })
      return best < 0 ? at : best
    }
    // A rail of months pages by month: to the newest day with events of the next month that has any.
    const byMonths = (offset: number): number => {
      const rows = this.monthRows()
      for (
        let row = this.activeMonthIndex() + offset;
        row >= 0 && row < rows.length;
        row += offset
      ) {
        const day = days.findIndex(day => day.count > 0 && day.key.startsWith(rows[row].key))
        if (day >= 0) return targets.indexOf(day)
      }
      return at
    }
    const months = this.showsMonths()
    const steps: Partial<Record<string, number>> = {
      ArrowDown: at + 1,
      ArrowUp: at - 1,
      PageDown: months ? byMonths(1) : byDays(7),
      PageUp: months ? byMonths(-1) : byDays(-7),
      Home: 0,
      End: targets.length - 1,
    }
    const vertical = event.key === 'ArrowUp' || event.key === 'ArrowDown'
    const next = at < 0 && vertical ? 0 : steps[event.key]
    if (next === undefined || targets.length === 0) return
    event.preventDefault()
    const target = days[targets[Math.min(targets.length - 1, Math.max(0, next))]]
    if (target.key !== this.activeDay()) this.jump.emit(target.key)
  }

  /** Applies the latest pointer position once per frame. */
  private queue(event: PointerEvent): void {
    this.pendingY = this.offsetOf(event)
    if (this.frame) return
    // Hand-written: Signality has no animation-frame scheduling.
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
