/// <reference types="@vitest/browser-playwright" />
import { TestBed } from '@angular/core/testing'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cdp, page } from 'vitest/browser'
import type { ActivityLog, GitEntry } from '../../generated/activity'
import { ActivityDataService } from '../../services/activity-data.service'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { MockBackendTransport } from '../../testing/activity-fixtures'
import { openApp, type OpenApp } from '../../testing/app'

function day(date: number, names: readonly string[]): GitEntry[] {
  return names.map((name, i) => ({
    source: 'git',
    connection: 'default',
    id: name,
    hash: name,
    timestamp: new Date(2026, 4, date, 17, i * -2).toISOString(),
    message: name,
    author: 'Dev',
    repo: 'demo',
  }))
}

const newest = day(18, ['newest A', 'newest B', 'newest C', 'newest D'])
const middle = day(16, ['above removed', 'above B', 'keep reader', 'below moves', 'below removed'])
const oldest = day(14, ['oldest reader', 'oldest A', 'oldest B'])
const tail = day(
  12,
  Array.from({ length: 12 }, (_, index) => `tail ${index}`)
)
const base = [...newest, ...middle, ...oldest, ...tail]
const RANGE = 'from=2026-05-12&to=2026-05-18'
const TIMELINE = `/timeline?${RANGE}`
const insertAbove = day(18, ['new top A', 'new top B'])
const insertBelow = day(16, ['new lower A', 'new lower B'])
const changed = [
  ...insertAbove,
  ...newest,
  middle[1]!,
  middle[2]!,
  ...insertBelow,
  middle[3]!,
  ...oldest,
  ...tail,
]

function log(entries: GitEntry[]): ActivityLog {
  return {
    meta: {
      generated: '2026-05-18T20:00:00Z',
      start: new Date(2026, 4, 12).toISOString(),
      end: new Date(2026, 4, 19).toISOString(),
    },
    summary: {
      connections: [{ connection: { source: 'git', name: 'default' }, count: entries.length }],
      mergedCount: 0,
    },
    entries,
  }
}

const root = (): HTMLElement => document.querySelector('app-timeline .timeline-scroll')!
const entry = (id: string): HTMLElement | null =>
  root().querySelector(`[data-entry="git/default/${id}"]`)
const ghosts = (): HTMLElement[] => [...document.querySelectorAll<HTMLElement>('.timeline-ghost')]
const offset = (element: HTMLElement): number =>
  element.getBoundingClientRect().top - root().getBoundingClientRect().top
const line = (): number => parseFloat(getComputedStyle(root()).getPropertyValue('--day-header-top'))

function afterPaint(): Promise<void> {
  return new Promise(resolve => requestAnimationFrame(() => setTimeout(resolve)))
}

async function open(entries = base, height = 800): Promise<OpenApp> {
  await page.viewport(1280, height)
  const backend = new MockBackendTransport()
  backend.activityResponse = log(entries)
  backend.activityResponses = [log(entries)]
  const app = await openApp(TIMELINE, { backend })
  await expect.poll(() => entry(entries[0]!.id)).not.toBeNull()
  await expect.poll(() => document.querySelector('.day-rise')).toBeNull()
  const reader = entry('keep reader')
  if (reader) root().scrollTop += offset(reader) - line() - 8
  for (let i = 0; i < 3; i++) await afterPaint()
  return app
}

function publish(backend: MockBackendTransport, entries: GitEntry[]): void {
  backend.activityResponse = log(entries)
  backend.publish()
}

function slides(element: Element): boolean {
  return (element.matches('.entry') ? element.parentElement! : element)
    .getAnimations()
    .some(
      animation =>
        animation.effect instanceof KeyframeEffect &&
        animation.effect.getKeyframes().some(frame => 'transform' in frame)
    )
}

async function holdRemoval(): Promise<void> {
  await expect.poll(ghosts).not.toHaveLength(0)
  for (const ghost of ghosts()) {
    for (const animation of ghost.getAnimations()) animation.pause()
  }
}

function expectNoListPositionTransitions(): void {
  for (const box of root().querySelectorAll('.timeline, .day-group')) {
    expect(getComputedStyle(box).transitionProperty).toBe('none')
  }
}

async function reduce(on: boolean): Promise<void> {
  await cdp().send('Emulation.setEmulatedMedia', {
    features: [{ name: 'prefers-reduced-motion', value: on ? 'reduce' : 'no-preference' }],
  })
  await afterPaint()
}

afterEach(async () => {
  vi.restoreAllMocks()
  await reduce(false)
})

describe('Timeline collection motion', () => {
  it('slides rows and day headers that moved on screen, preserving the reader and isolating ghosts', async () => {
    const { backend } = await open()
    const reader = entry('keep reader')!
    const before = offset(reader)
    const scrollWrites = vi.spyOn(root(), 'scrollTop', 'set')
    reader.focus({ preventScroll: true })
    const below = entry('below moves')!
    const header = root().querySelector('[data-day="2026-05-14"] .day-header')!
    const samples: number[] = []
    const sampling = (async (): Promise<void> => {
      for (let i = 0; i < 18; i++) {
        await afterPaint()
        samples.push(Math.abs(offset(reader) - before))
      }
    })()
    publish(backend, changed)
    await expect.poll(() => slides(below) && slides(header)).toBe(true)
    const writesAtStart = scrollWrites.mock.calls.length
    expect(writesAtStart).toBeGreaterThan(0)
    const ghost = ghosts().find(node => node.textContent?.includes('below removed'))!
    expect(ghost).toBeDefined()
    expect(ghost.closest('[role="feed"]')).toBeNull()
    expect(ghost.closest('[aria-hidden="true"][inert]')).not.toBeNull()
    expect(ghost.querySelector('[aria-posinset], [aria-setsize]')).toBeNull()
    ghost.querySelector('button')!.focus()
    expect(document.activeElement).toBe(reader)
    expect(page.getByRole('button', { name: /below removed/i }).query()).toBeNull()
    const added = entry('new lower A')!.parentElement!
    expect(added.getAnimations().length).toBeGreaterThan(0)
    await sampling
    expect(scrollWrites.mock.calls.length).toBe(writesAtStart)
    expect(Math.max(...samples)).toBeLessThanOrEqual(1)
    await expect.poll(ghosts).toHaveLength(0)
    expect(slides(below)).toBe(false)
    expect(getComputedStyle(below.parentElement!).transform).toBe('none')
  })

  it('fades a removed day header outside the feed and a new day header inside it', async () => {
    const { backend } = await open()
    publish(backend, [...newest, ...middle, ...day(15, ['new day item']), ...tail])
    await expect.poll(() => ghosts().some(node => !!node.querySelector('.day-header'))).toBe(true)
    const ghost = ghosts().find(node => !!node.querySelector('.day-header'))!
    expect(ghost.closest('[role="feed"]')).toBeNull()
    expect(ghost.getAnimations()).toHaveLength(1)
    const incoming = root().querySelector('[data-day="2026-05-15"] .day-header')!
    expect(incoming.getAnimations()).toHaveLength(1)
    await expect.poll(ghosts).toHaveLength(0)
  })

  it('pauses a collection in reduced motion and while a filter edit is settling', async () => {
    const { backend } = await open()
    publish(backend, changed)
    await holdRemoval()
    await reduce(true)
    await expect.poll(ghosts).toHaveLength(0)
    expectNoListPositionTransitions()
    const animate = vi.spyOn(HTMLElement.prototype, 'animate')
    publish(backend, base)
    await expect.poll(() => entry('below removed')).not.toBeNull()
    expect(ghosts()).toHaveLength(0)
    expect(animate).not.toHaveBeenCalled()
    await reduce(false)
    TestBed.inject(ActivityViewStateService).setSearchQuery('keep')
    publish(backend, changed)
    await expect.poll(() => root().querySelectorAll('.entry').length).toBe(1)
    expect(ghosts()).toHaveLength(0)
    expect(animate).not.toHaveBeenCalled()
  })

  it('clears removals when the reader wheels through an animating update', async () => {
    const { backend } = await open()
    publish(backend, changed)
    await holdRemoval()
    const before = root().scrollTop
    const { left, top, width, height } = root().getBoundingClientRect()
    await cdp().send('Input.dispatchMouseEvent', {
      type: 'mouseWheel',
      x: left + width / 2,
      y: top + height / 2,
      deltaX: 0,
      deltaY: 100,
    })
    await expect.poll(() => root().scrollTop).toBeGreaterThan(before)
    await expect.poll(ghosts).toHaveLength(0)
  })

  it('cancels in-flight list motion on a day-index jump during a collection', async () => {
    const { backend } = await open()
    const partial = log(changed)
    backend.activityResponse = {
      ...partial,
      meta: { ...partial.meta, pending: [{ source: 'jira', name: 'default' }] },
    }
    backend.publish()
    await holdRemoval()
    expect(TestBed.inject(ActivityDataService).collectionStatus().kind).toBe('collecting')
    const row = entry('below moves')!.parentElement!
    const move = row.getAnimations()[0]!
    move.pause()
    const before = root().scrollTop
    const index = root().querySelector('[role="slider"]')!
    index.dispatchEvent(
      new KeyboardEvent('keydown', { key: 'End', bubbles: true, cancelable: true })
    )
    await expect.poll(() => root().scrollTop).toBeGreaterThan(before)
    expect(ghosts()).toHaveLength(0)
    expect(move.playState).toBe('idle')
    publish(backend, changed)
  })

  it('clears motion while the router holds the detached Timeline', async () => {
    const { backend, router } = await open()
    const animate = HTMLElement.prototype.animate
    vi.spyOn(HTMLElement.prototype, 'animate').mockImplementation(function (
      this: HTMLElement,
      keyframes,
      options
    ) {
      const animation = animate.call(this, keyframes, options)
      animation.pause()
      return animation
    })
    publish(backend, changed)
    await expect.poll(ghosts).not.toHaveLength(0)
    const scroller = root()
    const inFlight = scroller
      .getAnimations({ subtree: true })
      .filter(
        animation => !(animation instanceof CSSAnimation || animation instanceof CSSTransition)
      )
    expect(inFlight).not.toHaveLength(0)
    const host = scroller.closest('app-timeline')!
    const ghost = ghosts()[0]!
    await router.navigateByUrl(`/week/2026-05-18?${RANGE}`)
    await expect.poll(() => scroller.isConnected).toBe(false)
    await expect.poll(() => host.contains(ghost)).toBe(false)
    expect(inFlight.map(animation => animation.playState)).toEqual(inFlight.map(() => 'idle'))
    publish(backend, base)
    const data = TestBed.inject(ActivityDataService)
    await expect.poll(() => data.allEntries().some(item => item.id === 'below removed')).toBe(true)
    await router.navigateByUrl(TIMELINE)
    await expect.poll(() => entry('below removed')).not.toBeNull()
    expect(root()).toBe(scroller)
    expect(ghosts()).toHaveLength(0)
    // Reattaching under the pointer can start the row's hover transitions.
    const animations = root().getAnimations({ subtree: true })
    expect(animations.filter(animation => !(animation instanceof CSSTransition))).toHaveLength(0)
    expectNoListPositionTransitions()
  })

  it('keeps a collection replacement of more than 32 on-screen items still', async () => {
    const many = (prefix: string): GitEntry[] =>
      day(
        18,
        Array.from({ length: 40 }, (_, i) => `${prefix} ${i}`)
      )
    const { backend } = await open(many('old'), 1600)
    const animate = vi.spyOn(HTMLElement.prototype, 'animate')
    publish(backend, many('new'))
    await expect.poll(() => entry('new 0')).not.toBeNull()
    expect(entry('old 0')).toBeNull()
    expect(animate).not.toHaveBeenCalled()
    expect(ghosts()).toHaveLength(0)
  })

  it('fades in an on-screen addition when most additions to a whole day are off screen', async () => {
    const names = Array.from({ length: 74 }, (_, i) => `row ${i}`)
    const rows = day(18, names)
    const { backend } = await open(rows.slice(0, 40))
    const visible = {
      ...day(18, ['visible addition'])[0]!,
      timestamp: new Date(2026, 4, 18, 20).toISOString(),
    }
    publish(backend, [visible, ...rows])
    await expect.poll(() => entry('visible addition')).not.toBeNull()
    expect(root().querySelectorAll('.entry')).toHaveLength(75)
    const animations = entry('visible addition')!.parentElement!.getAnimations()
    expect(
      animations.some(
        animation =>
          animation.effect instanceof KeyframeEffect &&
          animation.effect.getKeyframes().some(frame => frame['opacity'] === '0')
      )
    ).toBe(true)
  })
})
