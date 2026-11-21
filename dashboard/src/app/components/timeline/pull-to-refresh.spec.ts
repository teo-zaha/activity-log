import { describe, expect, it } from 'vitest'
import {
  PULL_EDGE_PX,
  PULL_LIMIT_PX,
  PULL_THRESHOLD_PX,
  indicatorDistance,
  pullMayStart,
  resisted,
  stepPull,
  type PullInput,
  type PullState,
  type PullStep,
} from './pull-to-refresh'

const TOUCH = 7
const X = 200
const Y = 300

/** The finger travel that brings the indicator to `distance`. */
const travelFor = (distance: number): number =>
  (distance * PULL_LIMIT_PX) / (PULL_LIMIT_PX - distance)

const press: PullInput = { kind: 'press', touch: TOUCH, x: X, y: Y }
const moveBy = (down: number, across = 0, touch = TOUCH): PullInput => ({
  kind: 'move',
  touch,
  x: X + across,
  y: Y + down,
})
const release: PullInput = { kind: 'release', touch: TOUCH }

/** Runs the inputs from idle; the steps taken, last last. */
function run(...inputs: PullInput[]): PullStep[] {
  const steps: PullStep[] = []
  let state: PullState = { kind: 'idle' }
  for (const input of inputs) {
    const step = stepPull(state, input)
    steps.push(step)
    state = step.state
  }
  return steps
}
const last = (steps: PullStep[]): PullStep => steps[steps.length - 1]!
const kinds = (steps: PullStep[]): PullState['kind'][] => steps.map(step => step.state.kind)
const effects = (steps: PullStep[]): PullStep['effect'][] => steps.map(step => step.effect)

describe('resisted', () => {
  it('follows the finger ever more slowly, never past the limit', () => {
    expect(resisted(0)).toBe(0)
    expect(resisted(-40)).toBe(0)
    expect(resisted(40)).toBeLessThan(40)
    expect(resisted(80) - resisted(40)).toBeLessThan(resisted(40))
    expect(resisted(10_000)).toBeLessThan(PULL_LIMIT_PX)
    expect(resisted(travelFor(PULL_THRESHOLD_PX))).toBeCloseTo(PULL_THRESHOLD_PX)
  })
})

describe('a pull', () => {
  it('collects once when released past the threshold', () => {
    const steps = run(press, moveBy(20), moveBy(travelFor(PULL_THRESHOLD_PX) + 5), release)

    expect(kinds(steps)).toEqual(['pressed', 'pulling', 'armed', 'refreshing'])
    expect(effects(steps)).toEqual(['none', 'none', 'announce', 'refresh'])
  })

  it('follows the finger with resistance', () => {
    const step = last(run(press, moveBy(50)))

    expect(step.state).toEqual({
      kind: 'pulling',
      touch: TOUCH,
      y: Y,
      distance: resisted(50),
      reached: false,
    })
    expect(indicatorDistance(step.state)).toBe(resisted(50))
  })

  it('collects nothing when released short of the threshold', () => {
    const steps = run(press, moveBy(travelFor(PULL_THRESHOLD_PX) - 5), release)

    expect(last(steps)).toEqual({ state: { kind: 'idle' }, effect: 'none' })
    expect(effects(steps)).not.toContain('refresh')
  })

  it('collects nothing when the finger goes back above the threshold before release', () => {
    const past = travelFor(PULL_THRESHOLD_PX) + 20
    const steps = run(press, moveBy(past), moveBy(10), release)

    expect(kinds(steps)).toEqual(['pressed', 'armed', 'pulling', 'idle'])
    expect(effects(steps)).not.toContain('refresh')
  })

  it('says once that releasing collects, however often the finger crosses the threshold', () => {
    const past = travelFor(PULL_THRESHOLD_PX) + 20
    const steps = run(press, moveBy(past), moveBy(10), moveBy(past), moveBy(past + 30), release)

    expect(effects(steps).filter(effect => effect === 'announce')).toHaveLength(1)
    expect(last(steps).effect).toBe('refresh')
  })

  it('keeps the indicator at the top while the finger goes back up past where it pressed', () => {
    const step = last(run(press, moveBy(30), moveBy(-40)))

    expect(step.state.kind).toBe('pulling')
    expect(indicatorDistance(step.state)).toBe(0)
  })

  it('leaves a touch that first moves up to the browser, which scrolls the list', () => {
    expect(kinds(run(press, moveBy(-10), moveBy(200), release))).toEqual([
      'pressed',
      'idle',
      'idle',
      'idle',
    ])
  })

  it('leaves a touch that first moves sideways to the browser and the system (edge swipe)', () => {
    expect(last(run(press, moveBy(10, 30), moveBy(200))).state.kind).toBe('idle')
    expect(last(run(press, moveBy(10, -30), moveBy(200))).state.kind).toBe('idle')
  })

  it('waits for a move that goes somewhere before it decides', () => {
    expect(kinds(run(press, moveBy(0), moveBy(12)))).toEqual(['pressed', 'pressed', 'pulling'])
  })

  it('follows only the finger that pressed', () => {
    const other = TOUCH + 1
    const steps = run(press, moveBy(200, 0, other), { kind: 'release', touch: other })

    expect(kinds(steps)).toEqual(['pressed', 'pressed', 'pressed'])
  })

  it('drops the pull when a second finger comes, as for a pinch', () => {
    const steps = run(press, moveBy(200), { ...press, touch: TOUCH + 1 }, moveBy(220), release)

    expect(kinds(steps)).toEqual(['pressed', 'armed', 'idle', 'idle', 'idle'])
    expect(effects(steps)).not.toContain('refresh')
  })

  it('drops the pull when the touch is cancelled or pulling is no longer offered', () => {
    expect(last(run(press, moveBy(200), { kind: 'cancel' }, release)).state.kind).toBe('idle')
  })
})

describe('a collection a pull started', () => {
  const refreshing: PullState = { kind: 'refreshing' }

  it('shows the indicator where it rests until the collection is over', () => {
    expect(indicatorDistance(refreshing)).toBe(PULL_THRESHOLD_PX)
    expect(stepPull(refreshing, { kind: 'settled' }).state.kind).toBe('idle')
  })

  it('starts no other pull meanwhile', () => {
    for (const input of [press, moveBy(300), release, { kind: 'cancel' } satisfies PullInput]) {
      expect(stepPull(refreshing, input)).toEqual({ state: refreshing, effect: 'none' })
    }
  })

  it('is not ended by a settled collection while no pull asked for one', () => {
    const pulling = last(run(press, moveBy(40))).state
    expect(stepPull(pulling, { kind: 'settled' }).state).toBe(pulling)
  })
})

describe('where a pull may start', () => {
  const width = 400
  const place = { scrollTop: 0, x: 200, width, onControls: false }

  it('at the top of the list, on the list', () => {
    expect(pullMayStart(place)).toBe(true)
    // iOS bounces the list at the top: its position is below zero then.
    expect(pullMayStart({ ...place, scrollTop: -12 })).toBe(true)
  })

  it('not further down the list, where the touch scrolls it', () => {
    expect(pullMayStart({ ...place, scrollTop: 1 })).toBe(false)
  })

  it('not on the filter or the day index', () => {
    expect(pullMayStart({ ...place, onControls: true })).toBe(false)
  })

  it('not at the screen edges, where the system swipes', () => {
    expect(pullMayStart({ ...place, x: PULL_EDGE_PX - 1 })).toBe(false)
    expect(pullMayStart({ ...place, x: width - PULL_EDGE_PX + 1 })).toBe(false)
    expect(pullMayStart({ ...place, x: PULL_EDGE_PX })).toBe(true)
  })
})
