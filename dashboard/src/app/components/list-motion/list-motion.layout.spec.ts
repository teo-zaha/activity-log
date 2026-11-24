import { signal } from '@angular/core'
import { TestBed } from '@angular/core/testing'
import { afterEach, beforeEach, describe, expect, it } from 'vitest'
import { ListMotion, type ListMotionFrame, type PositionedItem } from './list-motion'

const paused = signal(false)
let motion: ListMotion
let host: HTMLElement

beforeEach(() => {
  paused.set(false)
  host = document.createElement('div')
  document.body.append(host)
  motion = TestBed.runInInjectionContext(() => new ListMotion(host, paused))
})

afterEach(() => {
  motion.reset()
  host.remove()
})

function element(key: string): HTMLElement {
  const node = document.createElement('div')
  node.textContent = key
  host.append(node)
  return node
}

function items(rows: [string, HTMLElement, number][]): Map<string, PositionedItem> {
  return new Map(
    rows.map(([key, element, top]) => [
      key,
      { element, size: 20, topAt: (offset): number => top - offset },
    ])
  )
}

function frame(revision: number, mounted: ReadonlyMap<string, PositionedItem>): ListMotionFrame {
  return {
    revision,
    keys: mounted,
    items: mounted,
    beforeScroll: 0,
    scrollTop: 0,
    viewport: { top: 0, bottom: 200 },
    ghost: (item): HTMLElement => {
      const clone = element(item.element.textContent ?? '')
      clone.className = 'test-ghost'
      return clone
    },
  }
}

function firstTransform(node: HTMLElement): string {
  const effect = node.getAnimations()[0]?.effect
  if (!(effect instanceof KeyframeEffect)) throw new Error('Expected a movement animation')
  return String(effect.getKeyframes()[0]?.['transform'])
}

describe('keyed list motion', () => {
  it('adds, reorders and removes items in an ordinary list', async () => {
    const a = element('a')
    const b = element('b')
    motion.update(
      frame(
        1,
        items([
          ['a', a, 0],
          ['b', b, 20],
        ])
      )
    )
    a.remove()
    const c = element('c')
    motion.update(
      frame(
        2,
        items([
          ['b', b, 0],
          ['c', c, 20],
        ])
      )
    )

    expect(firstTransform(b)).toBe('translateY(20px)')
    expect(c.getAnimations()).toHaveLength(1)
    const ghost = host.querySelector('.test-ghost')!
    expect(ghost.textContent).toBe('a')
    for (const animation of host.getAnimations({ subtree: true })) animation.finish()
    await expect.poll(() => ghost.isConnected).toBe(false)
  })

  it('ignores virtual mounts and animates only changes left after the host scroll correction', () => {
    const a = element('a')
    const b = element('b')
    const c = element('c')
    const keys = new Set(['a', 'b', 'c'])
    motion.update({
      ...frame(
        1,
        items([
          ['a', a, 0],
          ['b', b, 20],
        ])
      ),
      keys,
    })
    motion.update({
      ...frame(
        1,
        items([
          ['b', b, 20],
          ['c', c, 40],
        ])
      ),
      keys,
    })
    expect(host.getAnimations({ subtree: true })).toHaveLength(0)
    motion.update({
      ...frame(
        2,
        items([
          ['b', b, 45],
          ['c', c, 80],
        ])
      ),
      keys,
      scrollTop: 20,
      anchor: 'b',
    })
    expect(b.getAnimations()).toHaveLength(0)
    expect(firstTransform(c)).toBe('translateY(-20px)')
    expect(host.querySelector('.test-ghost')).toBeNull()
  })

  it('retargets an unfinished transform from its current screen position', () => {
    const node = element('a')
    motion.update(frame(1, items([['a', node, 30]])))
    motion.update(frame(2, items([['a', node, 70]])))
    const running = node.getAnimations()[0]!
    running.pause()
    running.currentTime = 90
    const progress = running.effect!.getComputedTiming().progress!
    motion.update(frame(3, items([['a', node, 50]])))
    const offset = Number.parseFloat(firstTransform(node).slice('translateY('.length))
    expect(offset).toBeCloseTo(70 - 40 * (1 - progress) - 50, 4)
  })

  it('starts from the current node when a virtual row has remounted', () => {
    const old = element('a')
    motion.update(frame(1, items([['a', old, 20]])))
    motion.update(frame(2, items([['a', old, 50]])))
    const running = old.getAnimations()[0]!
    running.pause()
    running.currentTime = 40
    old.remove()
    const current = element('a')
    motion.update(frame(3, items([['a', current, 60]])))
    expect(current.getAnimations()).toHaveLength(0)
    motion.update(frame(4, items([['a', current, 80]])))
    expect(firstTransform(current)).toBe('translateY(-20px)')
  })

  it('leaves a large replacement still', () => {
    const before = Array.from({ length: 25 }, (_, i): [string, HTMLElement, number] => [
      `old-${i}`,
      element(`old-${i}`),
      i * 20,
    ])
    const viewport = { top: 0, bottom: 400 }
    motion.update({ ...frame(1, items(before)), viewport })
    for (const [, node] of before) node.remove()
    const after = Array.from({ length: 25 }, (_, i): [string, HTMLElement, number] => [
      `new-${i}`,
      element(`new-${i}`),
      i * 20,
    ])
    motion.update({ ...frame(2, items(after)), viewport })
    expect(host.getAnimations({ subtree: true })).toHaveLength(0)
    expect(host.querySelector('.test-ghost')).toBeNull()
  })

  it('animates on-screen additions and removals despite many off-screen changes', () => {
    const removed = element('visible removal')
    const offscreen = (prefix: string): [string, HTMLElement, number][] =>
      Array.from({ length: 34 }, (_, i) => {
        const key = `${prefix}-${i}`
        return [key, element(key), 200 + i * 20]
      })
    motion.update(frame(1, items([['removed', removed, 10], ...offscreen('old')])))
    const added = element('visible addition')
    motion.update(frame(2, items([['added', added, 10], ...offscreen('new')])))
    expect(added.getAnimations()).toHaveLength(1)
    expect([...host.querySelectorAll('.test-ghost')].map(node => node.textContent)).toEqual([
      'visible removal',
    ])
    expect(host.getAnimations({ subtree: true })).toHaveLength(2)
  })

  it('cancels motion when paused and resumes from the paused layout', () => {
    const node = element('a')
    motion.update(frame(1, items([['a', node, 30]])))
    motion.update(frame(2, items([['a', node, 70]])))
    expect(node.getAnimations()).toHaveLength(1)
    paused.set(true)
    motion.update(frame(3, items([['a', node, 40]])))
    expect(node.getAnimations()).toHaveLength(0)
    paused.set(false)
    motion.update(frame(4, items([['a', node, 50]])))
    expect(firstTransform(node)).toBe('translateY(-10px)')
  })

  it('leaves the list still without Web Animations', () => {
    Object.defineProperty(host, 'animate', { value: undefined })
    const node = element('a')
    motion.update(frame(1, items([['a', node, 0]])))
    motion.update(frame(2, items([['a', node, 30]])))
    expect(node.getAnimations()).toHaveLength(0)
  })
})
