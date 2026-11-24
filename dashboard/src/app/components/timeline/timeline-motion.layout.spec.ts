/// <reference types="@vitest/browser-playwright" />
import { TestBed } from '@angular/core/testing'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cdp, page } from 'vitest/browser'
import type { ActivityLog, GitEntry } from '../../generated/activity'
import { ActivityViewStateService } from '../../services/activity-view-state.service'
import { MockBackendTransport } from '../../testing/activity-fixtures'
import { openApp } from '../../testing/app'

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

async function open(): Promise<MockBackendTransport> {
  await page.viewport(1280, 800)
  const backend = new MockBackendTransport()
  backend.activityResponse = log(base)
  backend.activityResponses = [log(base)]
  await openApp('/timeline?from=2026-05-12&to=2026-05-18', { backend })
  await expect.poll(() => entry('keep reader')).not.toBeNull()
  await expect.poll(() => document.querySelector('.day-rise')).toBeNull()
  root().scrollTop += offset(entry('keep reader')!) - line() - 8
  for (let i = 0; i < 3; i++) await afterPaint()
  return backend
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
    const backend = await open()
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
    const backend = await open()
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
    const backend = await open()
    publish(backend, changed)
    await expect.poll(() => ghosts().length).toBeGreaterThan(0)
    await reduce(true)
    publish(backend, base)
    await expect.poll(() => entry('below removed')).not.toBeNull()
    expect(ghosts()).toHaveLength(0)
    expect(root().getAnimations({ subtree: true })).toHaveLength(0)
    await reduce(false)
    TestBed.inject(ActivityViewStateService).setSearchQuery('keep')
    publish(backend, changed)
    await expect.poll(() => root().querySelectorAll('.entry').length).toBe(1)
    expect(ghosts()).toHaveLength(0)
    expect(root().getAnimations({ subtree: true })).toHaveLength(0)
  })
})
